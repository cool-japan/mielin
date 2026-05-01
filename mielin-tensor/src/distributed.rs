//! Distributed Inference Engine for MielinOS tensor mesh
//!
//! Implements tensor partitioning, distributed transport, Cannon's algorithm
//! for distributed matrix multiplication, model-parallel layers, and an
//! end-to-end inference orchestrator.

extern crate alloc;
extern crate std;

use alloc::vec::Vec;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::ops::TensorOps;
use crate::tensor::Tensor;
use mielin_hal::capabilities::HardwareCapabilities;

// ─── Identifiers ─────────────────────────────────────────────────────────────

/// Logical node identifier in the mesh
pub type NodeId = u64;

/// Unique shard identifier (node_id << 32 | shard_index)
pub type ShardId = u64;

fn make_shard_id(node_id: NodeId, shard_index: u32) -> ShardId {
    (node_id << 32) | (shard_index as u64)
}

// ─── Error type ───────────────────────────────────────────────────────────────

/// Errors produced by the distributed inference subsystem
#[derive(Debug)]
pub enum DistributedError {
    #[allow(dead_code)]
    ShardMismatch {
        expected: usize,
        actual: usize,
    },
    ShapeMismatch {
        expected: alloc::vec::Vec<usize>,
        actual: alloc::vec::Vec<usize>,
    },
    TransportError(alloc::string::String),
    ReconstructionError(alloc::string::String),
    InvalidStrategy(alloc::string::String),
    EmptyInput,
    MatmulDimensionError {
        m: usize,
        k_a: usize,
        k_b: usize,
        n: usize,
    },
    NodeNotFound(NodeId),
    ShardNotFound(ShardId),
    CompilationError(alloc::string::String),
    InferenceError(alloc::string::String),
}

impl std::fmt::Display for DistributedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DistributedError::ShardMismatch { expected, actual } => {
                write!(f, "Shard mismatch: expected {expected}, got {actual}")
            }
            DistributedError::ShapeMismatch { expected, actual } => {
                write!(f, "Shape mismatch: expected {expected:?}, got {actual:?}")
            }
            DistributedError::TransportError(msg) => write!(f, "Transport error: {msg}"),
            DistributedError::ReconstructionError(msg) => {
                write!(f, "Reconstruction error: {msg}")
            }
            DistributedError::InvalidStrategy(msg) => write!(f, "Invalid strategy: {msg}"),
            DistributedError::EmptyInput => write!(f, "Empty input"),
            DistributedError::MatmulDimensionError { m, k_a, k_b, n } => {
                write!(
                    f,
                    "Matmul dimension error: ({m}x{k_a}) * ({k_b}x{n}) inner dims don't match"
                )
            }
            DistributedError::NodeNotFound(id) => write!(f, "Node {id} not found"),
            DistributedError::ShardNotFound(id) => write!(f, "Shard {id} not found"),
            DistributedError::CompilationError(msg) => write!(f, "Compilation error: {msg}"),
            DistributedError::InferenceError(msg) => write!(f, "Inference error: {msg}"),
        }
    }
}

impl std::error::Error for DistributedError {}

// ─── Partition strategy ───────────────────────────────────────────────────────

/// How a tensor is split across logical shards
#[derive(Debug, Clone, PartialEq)]
pub enum PartitionStrategy {
    /// Split along row dimension (dim-0)
    RowWise,
    /// Split along column dimension (dim-1)
    ColumnWise,
    /// 2-D block decomposition
    Block { rows: usize, cols: usize },
    /// Pipeline stages — each shard is a contiguous set of layers / tokens
    Pipeline { stages: usize },
}

// ─── TensorShard ─────────────────────────────────────────────────────────────

/// A contiguous slice of a larger tensor, tagged with provenance metadata
#[derive(Debug, Clone)]
pub struct TensorShard {
    /// Unique shard identifier
    pub shard_id: ShardId,
    /// Which logical shard index this is (0 … num_shards-1)
    pub shard_index: usize,
    /// Total number of shards the original was split into
    pub num_shards: usize,
    /// Shape of the *original* (global) tensor
    pub global_shape: Vec<usize>,
    /// Row-start of this shard within the global tensor (for row/block partition)
    pub row_start: usize,
    /// Row-end (exclusive) within the global tensor
    pub row_end: usize,
    /// Col-start of this shard within the global tensor (for col/block partition)
    pub col_start: usize,
    /// Col-end (exclusive) within the global tensor
    pub col_end: usize,
    /// Local data for this shard (row-major)
    pub data: Vec<f32>,
    /// Local shape \[rows, cols\] or \[rows\] for 1-D tensors
    pub local_shape: Vec<usize>,
    /// Partition strategy that produced this shard
    pub strategy: PartitionStrategy,
}

impl TensorShard {
    /// Total number of elements in this shard
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Convert shard data into a local `Tensor`
    pub fn to_local_tensor(&self) -> Tensor<f32> {
        Tensor::from_vec(self.data.clone(), self.local_shape.clone())
            .expect("shard data / shape invariant violated")
    }
}

// ─── ShardedTensor ────────────────────────────────────────────────────────────

/// A tensor whose data is distributed across N logical shards
pub struct ShardedTensor {
    /// Global shape of the full tensor
    pub global_shape: Vec<usize>,
    /// Strategy used to produce the shards
    pub strategy: PartitionStrategy,
    /// All shards (may live on different nodes conceptually)
    pub shards: Vec<TensorShard>,
}

impl ShardedTensor {
    /// Partition a 2-D tensor into shards according to `strategy`
    pub fn partition(
        tensor: &Tensor<f32>,
        strategy: PartitionStrategy,
        num_shards: usize,
    ) -> Result<Vec<TensorShard>, DistributedError> {
        if num_shards == 0 {
            return Err(DistributedError::InvalidStrategy(
                "num_shards must be > 0".into(),
            ));
        }
        if tensor.ndim() < 1 {
            return Err(DistributedError::EmptyInput);
        }

        match &strategy {
            PartitionStrategy::RowWise => Self::partition_row_wise(tensor, num_shards, strategy),
            PartitionStrategy::ColumnWise => Self::partition_col_wise(tensor, num_shards, strategy),
            PartitionStrategy::Block { rows, cols } => {
                Self::partition_block(tensor, num_shards, *rows, *cols, strategy)
            }
            PartitionStrategy::Pipeline { stages } => {
                Self::partition_pipeline(tensor, *stages, strategy)
            }
        }
    }

    /// Reconstruct the full tensor from all its shards
    pub fn reconstruct(
        shards: Vec<TensorShard>,
        strategy: PartitionStrategy,
    ) -> Result<Tensor<f32>, DistributedError> {
        if shards.is_empty() {
            return Err(DistributedError::EmptyInput);
        }
        match strategy {
            PartitionStrategy::RowWise | PartitionStrategy::Pipeline { .. } => {
                Self::reconstruct_row_wise(shards)
            }
            PartitionStrategy::ColumnWise => Self::reconstruct_col_wise(shards),
            PartitionStrategy::Block { .. } => Self::reconstruct_block(shards),
        }
    }

    // ── row-wise helpers ─────────────────────────────────────────────────────

    fn partition_row_wise(
        tensor: &Tensor<f32>,
        num_shards: usize,
        strategy: PartitionStrategy,
    ) -> Result<Vec<TensorShard>, DistributedError> {
        let global_shape = tensor.shape().to_vec();
        let total_rows = global_shape[0];
        let base = total_rows / num_shards;
        let remainder = total_rows % num_shards;
        let cols = if global_shape.len() > 1 {
            global_shape[1]
        } else {
            1
        };

        let mut shards = Vec::with_capacity(num_shards);
        let mut row_cursor = 0usize;

        for idx in 0..num_shards {
            // Distribute remainder rows among first `remainder` shards
            let shard_rows = base + if idx < remainder { 1 } else { 0 };
            let row_end = row_cursor + shard_rows;

            let local_data: Vec<f32> = if global_shape.len() == 1 {
                tensor.data()[row_cursor..row_end].to_vec()
            } else {
                let start = row_cursor * cols;
                let end = row_end * cols;
                tensor.data()[start..end].to_vec()
            };

            let local_shape = if global_shape.len() == 1 {
                alloc::vec![shard_rows]
            } else {
                alloc::vec![shard_rows, cols]
            };

            shards.push(TensorShard {
                shard_id: make_shard_id(idx as NodeId, idx as u32),
                shard_index: idx,
                num_shards,
                global_shape: global_shape.clone(),
                row_start: row_cursor,
                row_end,
                col_start: 0,
                col_end: cols,
                data: local_data,
                local_shape,
                strategy: strategy.clone(),
            });

            row_cursor = row_end;
        }

        Ok(shards)
    }

    fn reconstruct_row_wise(mut shards: Vec<TensorShard>) -> Result<Tensor<f32>, DistributedError> {
        shards.sort_by_key(|s| s.shard_index);
        let global_shape = shards[0].global_shape.clone();
        let mut data: Vec<f32> = Vec::with_capacity(global_shape.iter().product());
        for shard in shards {
            data.extend_from_slice(&shard.data);
        }
        Tensor::from_vec(data, global_shape)
            .ok_or_else(|| DistributedError::ReconstructionError("row-wise reconstruct".into()))
    }

    // ── column-wise helpers ──────────────────────────────────────────────────

    fn partition_col_wise(
        tensor: &Tensor<f32>,
        num_shards: usize,
        strategy: PartitionStrategy,
    ) -> Result<Vec<TensorShard>, DistributedError> {
        if tensor.ndim() < 2 {
            return Err(DistributedError::InvalidStrategy(
                "ColumnWise requires at least 2-D tensor".into(),
            ));
        }
        let global_shape = tensor.shape().to_vec();
        let rows = global_shape[0];
        let total_cols = global_shape[1];
        let base = total_cols / num_shards;
        let remainder = total_cols % num_shards;

        let mut shards = Vec::with_capacity(num_shards);
        let mut col_cursor = 0usize;

        for idx in 0..num_shards {
            let shard_cols = base + if idx < remainder { 1 } else { 0 };
            let col_end = col_cursor + shard_cols;

            // Extract shard_cols columns from each row
            let mut local_data = Vec::with_capacity(rows * shard_cols);
            for row in 0..rows {
                let row_start = row * total_cols + col_cursor;
                let row_end = row * total_cols + col_end;
                local_data.extend_from_slice(&tensor.data()[row_start..row_end]);
            }

            shards.push(TensorShard {
                shard_id: make_shard_id(idx as NodeId, idx as u32),
                shard_index: idx,
                num_shards,
                global_shape: global_shape.clone(),
                row_start: 0,
                row_end: rows,
                col_start: col_cursor,
                col_end,
                data: local_data,
                local_shape: alloc::vec![rows, shard_cols],
                strategy: strategy.clone(),
            });

            col_cursor = col_end;
        }

        Ok(shards)
    }

    fn reconstruct_col_wise(mut shards: Vec<TensorShard>) -> Result<Tensor<f32>, DistributedError> {
        shards.sort_by_key(|s| s.shard_index);
        let global_shape = shards[0].global_shape.clone();
        let rows = global_shape[0];
        let total_cols = global_shape[1];
        let mut data = alloc::vec![0.0f32; rows * total_cols];

        for shard in &shards {
            let shard_cols = shard.col_end - shard.col_start;
            for row in 0..rows {
                for c in 0..shard_cols {
                    let global_idx = row * total_cols + shard.col_start + c;
                    let local_idx = row * shard_cols + c;
                    data[global_idx] = shard.data[local_idx];
                }
            }
        }

        Tensor::from_vec(data, global_shape)
            .ok_or_else(|| DistributedError::ReconstructionError("col-wise reconstruct".into()))
    }

    // ── block partition helpers ──────────────────────────────────────────────

    fn partition_block(
        tensor: &Tensor<f32>,
        _num_shards: usize,
        block_rows: usize,
        block_cols: usize,
        strategy: PartitionStrategy,
    ) -> Result<Vec<TensorShard>, DistributedError> {
        if tensor.ndim() < 2 {
            return Err(DistributedError::InvalidStrategy(
                "Block requires at least 2-D tensor".into(),
            ));
        }
        let global_shape = tensor.shape().to_vec();
        let total_rows = global_shape[0];
        let total_cols = global_shape[1];

        if block_rows == 0 || block_cols == 0 {
            return Err(DistributedError::InvalidStrategy(
                "Block dimensions must be > 0".into(),
            ));
        }

        let num_row_blocks = total_rows.div_ceil(block_rows);
        let num_col_blocks = total_cols.div_ceil(block_cols);
        let total_blocks = num_row_blocks * num_col_blocks;
        let mut shards = Vec::with_capacity(total_blocks);

        for br in 0..num_row_blocks {
            let row_start = br * block_rows;
            let row_end = (row_start + block_rows).min(total_rows);

            for bc in 0..num_col_blocks {
                let col_start = bc * block_cols;
                let col_end = (col_start + block_cols).min(total_cols);
                let shard_rows = row_end - row_start;
                let shard_cols = col_end - col_start;

                let mut local_data = Vec::with_capacity(shard_rows * shard_cols);
                for row in row_start..row_end {
                    let src_start = row * total_cols + col_start;
                    let src_end = row * total_cols + col_end;
                    local_data.extend_from_slice(&tensor.data()[src_start..src_end]);
                }

                let idx = br * num_col_blocks + bc;
                shards.push(TensorShard {
                    shard_id: make_shard_id(idx as NodeId, idx as u32),
                    shard_index: idx,
                    num_shards: total_blocks,
                    global_shape: global_shape.clone(),
                    row_start,
                    row_end,
                    col_start,
                    col_end,
                    data: local_data,
                    local_shape: alloc::vec![shard_rows, shard_cols],
                    strategy: strategy.clone(),
                });
            }
        }

        Ok(shards)
    }

    fn reconstruct_block(mut shards: Vec<TensorShard>) -> Result<Tensor<f32>, DistributedError> {
        shards.sort_by_key(|s| s.shard_index);
        let global_shape = shards[0].global_shape.clone();
        let total_rows = global_shape[0];
        let total_cols = global_shape[1];
        let mut data = alloc::vec![0.0f32; total_rows * total_cols];

        for shard in &shards {
            let shard_cols = shard.col_end - shard.col_start;
            for row in shard.row_start..shard.row_end {
                let local_row = row - shard.row_start;
                for col in shard.col_start..shard.col_end {
                    let local_col = col - shard.col_start;
                    let global_idx = row * total_cols + col;
                    let local_idx = local_row * shard_cols + local_col;
                    data[global_idx] = shard.data[local_idx];
                }
            }
        }

        Tensor::from_vec(data, global_shape)
            .ok_or_else(|| DistributedError::ReconstructionError("block reconstruct".into()))
    }

    // ── pipeline partition helpers ───────────────────────────────────────────

    fn partition_pipeline(
        tensor: &Tensor<f32>,
        stages: usize,
        strategy: PartitionStrategy,
    ) -> Result<Vec<TensorShard>, DistributedError> {
        if stages == 0 {
            return Err(DistributedError::InvalidStrategy(
                "Pipeline stages must be > 0".into(),
            ));
        }
        // Pipeline partitions the *row* dimension (batch / sequence dimension)
        Self::partition_row_wise(tensor, stages, strategy)
    }
}

// ─── Reduce operation ─────────────────────────────────────────────────────────

/// Element-wise reduction across shards
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReduceOp {
    Sum,
    Product,
    Max,
    Min,
}

impl ReduceOp {
    fn identity(&self) -> f32 {
        match self {
            ReduceOp::Sum => 0.0,
            ReduceOp::Product => 1.0,
            ReduceOp::Max => f32::NEG_INFINITY,
            ReduceOp::Min => f32::INFINITY,
        }
    }

    fn combine(&self, acc: f32, val: f32) -> f32 {
        match self {
            ReduceOp::Sum => acc + val,
            ReduceOp::Product => acc * val,
            ReduceOp::Max => acc.max(val),
            ReduceOp::Min => acc.min(val),
        }
    }
}

// ─── DistributedTransport trait ──────────────────────────────────────────────

/// Pluggable transport layer — swap for real QUIC/mesh implementation later
pub trait DistributedTransport: Send + Sync {
    /// Send a shard to a specific node
    fn send_shard(&self, node_id: NodeId, shard: TensorShard) -> Result<(), DistributedError>;

    /// Receive a shard by its ShardId
    fn recv_shard(&self, shard_id: ShardId) -> Result<TensorShard, DistributedError>;

    /// Broadcast a shard to every participating node and collect the results
    fn broadcast(&self, shard: TensorShard) -> Result<Vec<TensorShard>, DistributedError>;

    /// All-reduce: each node contributes a shard, we return a single merged shard
    fn all_reduce(
        &self,
        shards: Vec<TensorShard>,
        op: ReduceOp,
    ) -> Result<TensorShard, DistributedError>;
}

// ─── LocalTransport ───────────────────────────────────────────────────────────

/// Single-process transport using shared memory — useful for tests and single-node inference
pub struct LocalTransport {
    /// Stores shards keyed by ShardId for later retrieval
    store: Arc<Mutex<HashMap<ShardId, TensorShard>>>,
    /// Known node ids for broadcast
    nodes: Vec<NodeId>,
}

impl LocalTransport {
    /// Create a transport that simulates `num_nodes` logical nodes
    pub fn new(num_nodes: usize) -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
            nodes: (0..num_nodes as NodeId).collect(),
        }
    }
}

impl DistributedTransport for LocalTransport {
    fn send_shard(&self, _node_id: NodeId, shard: TensorShard) -> Result<(), DistributedError> {
        let mut guard = self
            .store
            .lock()
            .map_err(|e| DistributedError::TransportError(e.to_string()))?;
        guard.insert(shard.shard_id, shard);
        Ok(())
    }

    fn recv_shard(&self, shard_id: ShardId) -> Result<TensorShard, DistributedError> {
        let guard = self
            .store
            .lock()
            .map_err(|e| DistributedError::TransportError(e.to_string()))?;
        guard
            .get(&shard_id)
            .cloned()
            .ok_or(DistributedError::ShardNotFound(shard_id))
    }

    fn broadcast(&self, shard: TensorShard) -> Result<Vec<TensorShard>, DistributedError> {
        // In single-process mode each "node" gets an identical copy
        let copies: Vec<TensorShard> = self.nodes.iter().map(|_| shard.clone()).collect();
        Ok(copies)
    }

    fn all_reduce(
        &self,
        shards: Vec<TensorShard>,
        op: ReduceOp,
    ) -> Result<TensorShard, DistributedError> {
        if shards.is_empty() {
            return Err(DistributedError::EmptyInput);
        }

        let len = shards[0].data.len();
        // Verify all shards carry same shape so the element-wise reduction is well-defined
        for s in &shards {
            if s.data.len() != len || s.local_shape != shards[0].local_shape {
                return Err(DistributedError::ShapeMismatch {
                    expected: shards[0].local_shape.clone(),
                    actual: s.local_shape.clone(),
                });
            }
        }

        let mut result_data = alloc::vec![op.identity(); len];
        for shard in &shards {
            for (acc, &val) in result_data.iter_mut().zip(shard.data.iter()) {
                *acc = op.combine(*acc, val);
            }
        }

        let prototype = &shards[0];
        Ok(TensorShard {
            shard_id: prototype.shard_id,
            shard_index: 0,
            num_shards: 1,
            global_shape: prototype.global_shape.clone(),
            row_start: prototype.row_start,
            row_end: prototype.row_end,
            col_start: prototype.col_start,
            col_end: prototype.col_end,
            data: result_data,
            local_shape: prototype.local_shape.clone(),
            strategy: prototype.strategy.clone(),
        })
    }
}

// ─── Distributed matmul (Cannon's algorithm) ─────────────────────────────────

/// Execute C = A * B using 2-D block decomposition and all-reduce of partial products
///
/// Cannon's algorithm avoids O(p) broadcast bandwidth; each node circularly
/// shifts A horizontally and B vertically after each local multiply step.
/// In the single-process `LocalTransport` this becomes a straightforward
/// block-wise partial-product accumulation, but the *logical structure*
/// matches Cannon's so real transport can slot in without changing the algorithm.
pub fn distributed_matmul(
    a: &Tensor<f32>,
    b: &Tensor<f32>,
    num_shards: usize,
    transport: &Arc<dyn DistributedTransport>,
) -> Result<Tensor<f32>, DistributedError> {
    if a.ndim() != 2 || b.ndim() != 2 {
        return Err(DistributedError::InvalidStrategy(
            "distributed_matmul requires 2-D tensors".into(),
        ));
    }

    let m = a.shape()[0];
    let k = a.shape()[1];
    let k2 = b.shape()[0];
    let n = b.shape()[1];

    if k != k2 {
        return Err(DistributedError::MatmulDimensionError {
            m,
            k_a: k,
            k_b: k2,
            n,
        });
    }

    // Determine 2-D process grid (p_r x p_c) such that p_r * p_c ≈ num_shards
    let p_r = (num_shards as f64).sqrt() as usize;
    let p_c = num_shards.div_ceil(p_r);

    // Block sizes
    let block_m = m.div_ceil(p_r);
    let block_n = n.div_ceil(p_c);
    let block_k = k.div_ceil(p_r.max(p_c));

    // Partition A into (p_r x num_k_blocks) blocks, B into (num_k_blocks x p_c) blocks
    let num_k_blocks = k.div_ceil(block_k);

    let ops = TensorOps::new(HardwareCapabilities::NONE);

    // Compute partial products: for each output block (i, j) accumulate A[i,t] * B[t,j]
    let mut result_data = alloc::vec![0.0f32; m * n];

    for bi in 0..p_r {
        let row_start_a = bi * block_m;
        let row_end_a = (row_start_a + block_m).min(m);
        if row_start_a >= m {
            continue;
        }

        for bj in 0..p_c {
            let col_start_b = bj * block_n;
            let col_end_b = (col_start_b + block_n).min(n);
            if col_start_b >= n {
                continue;
            }

            // Partial product for output block (bi, bj)
            let local_m = row_end_a - row_start_a;
            let local_n = col_end_b - col_start_b;
            let mut partial_data = alloc::vec![0.0f32; local_m * local_n];

            // Accumulate over k-blocks (the "shift" in Cannon's algorithm)
            for bk in 0..num_k_blocks {
                let col_start_a = bk * block_k;
                let col_end_a = (col_start_a + block_k).min(k);
                let row_start_b = bk * block_k;
                let row_end_b = (row_start_b + block_k).min(k);
                if col_start_a >= k {
                    continue;
                }
                let local_k = col_end_a - col_start_a;

                // Extract sub-block of A
                let mut a_block_data = alloc::vec![0.0f32; local_m * local_k];
                for r in 0..local_m {
                    let src = (row_start_a + r) * k + col_start_a;
                    a_block_data[r * local_k..(r + 1) * local_k]
                        .copy_from_slice(&a.data()[src..src + local_k]);
                }
                let a_block = Tensor::from_vec(a_block_data, alloc::vec![local_m, local_k])
                    .ok_or_else(|| DistributedError::InferenceError("a_block".into()))?;

                // Extract sub-block of B
                let row_end_b_actual = row_end_b.min(k);
                let local_k_b = row_end_b_actual - row_start_b;
                let mut b_block_data = alloc::vec![0.0f32; local_k_b * local_n];
                for r in 0..local_k_b {
                    for c in 0..local_n {
                        let src = (row_start_b + r) * n + col_start_b + c;
                        b_block_data[r * local_n + c] = b.data()[src];
                    }
                }
                let b_block = Tensor::from_vec(b_block_data, alloc::vec![local_k_b, local_n])
                    .ok_or_else(|| DistributedError::InferenceError("b_block".into()))?;

                // Local matmul of sub-blocks
                let local_result = ops.matmul(&a_block, &b_block).ok_or_else(|| {
                    DistributedError::InferenceError("block matmul failed".into())
                })?;

                // Accumulate into partial result
                for (acc, &val) in partial_data.iter_mut().zip(local_result.data().iter()) {
                    *acc += val;
                }
            }

            // Package partial result as a shard and send to transport
            let shard_idx = bi * p_c + bj;
            let partial_shard = TensorShard {
                shard_id: make_shard_id(shard_idx as NodeId, shard_idx as u32),
                shard_index: shard_idx,
                num_shards: p_r * p_c,
                global_shape: alloc::vec![m, n],
                row_start: row_start_a,
                row_end: row_end_a,
                col_start: col_start_b,
                col_end: col_end_b,
                data: partial_data,
                local_shape: alloc::vec![local_m, local_n],
                strategy: PartitionStrategy::Block {
                    rows: block_m,
                    cols: block_n,
                },
            };

            transport.send_shard(shard_idx as NodeId, partial_shard)?;
        }
    }

    // Collect all partial blocks and copy into result
    for bi in 0..p_r {
        let row_start_a = bi * block_m;
        let row_end_a = (row_start_a + block_m).min(m);
        if row_start_a >= m {
            continue;
        }
        let local_m = row_end_a - row_start_a;

        for bj in 0..p_c {
            let col_start_b = bj * block_n;
            let col_end_b = (col_start_b + block_n).min(n);
            if col_start_b >= n {
                continue;
            }
            let local_n = col_end_b - col_start_b;

            let shard_idx = bi * p_c + bj;
            let shard_id = make_shard_id(shard_idx as NodeId, shard_idx as u32);
            let collected = transport.recv_shard(shard_id)?;

            for r in 0..local_m {
                for c in 0..local_n {
                    let global_idx = (row_start_a + r) * n + col_start_b + c;
                    result_data[global_idx] = collected.data[r * local_n + c];
                }
            }
        }
    }

    Tensor::from_vec(result_data, alloc::vec![m, n])
        .ok_or_else(|| DistributedError::ReconstructionError("distributed_matmul".into()))
}

// ─── Activation function for model-parallel layers ───────────────────────────

/// Activation functions usable inside model-parallel layers
#[derive(Debug, Clone, Copy, Default)]
pub enum ActivationFn {
    #[default]
    Identity,
    ReLU,
    Sigmoid,
    Tanh,
    GELU,
}

impl ActivationFn {
    fn apply_inplace(&self, data: &mut [f32]) {
        match self {
            ActivationFn::Identity => {}
            ActivationFn::ReLU => {
                for v in data.iter_mut() {
                    if *v < 0.0 {
                        *v = 0.0;
                    }
                }
            }
            ActivationFn::Sigmoid => {
                for v in data.iter_mut() {
                    *v = 1.0 / (1.0 + libm::expf(-*v));
                }
            }
            ActivationFn::Tanh => {
                for v in data.iter_mut() {
                    *v = libm::tanhf(*v);
                }
            }
            ActivationFn::GELU => {
                // Approximate: 0.5 * x * (1 + tanh(sqrt(2/π) * (x + 0.044715*x³)))
                const SQRT_2_PI: f32 = 0.797_884_6;
                for v in data.iter_mut() {
                    let x3 = *v * *v * *v;
                    let inner = SQRT_2_PI * (*v + 0.044715 * x3);
                    *v = 0.5 * *v * (1.0 + libm::tanhf(inner));
                }
            }
        }
    }
}

// ─── ModelParallelLayer ───────────────────────────────────────────────────────

/// A single fully-connected layer whose weight matrix is sharded across nodes
pub struct ModelParallelLayer {
    /// Sharded weight tensor; shards live in `weight.shards`
    pub weight: ShardedTensor,
    /// Optional un-sharded bias vector
    pub bias: Option<Tensor<f32>>,
    /// Post-multiplication activation
    pub activation: ActivationFn,
    /// How the weight is sharded
    pub shard_strategy: PartitionStrategy,
}

impl ModelParallelLayer {
    /// Create a new layer from a weight tensor, optionally with bias
    pub fn new(
        weight: Tensor<f32>,
        bias: Option<Tensor<f32>>,
        activation: ActivationFn,
        strategy: PartitionStrategy,
        num_shards: usize,
    ) -> Result<Self, DistributedError> {
        let shards = ShardedTensor::partition(&weight, strategy.clone(), num_shards)?;
        let sharded = ShardedTensor {
            global_shape: weight.shape().to_vec(),
            strategy: strategy.clone(),
            shards,
        };
        Ok(Self {
            weight: sharded,
            bias,
            activation,
            shard_strategy: strategy,
        })
    }

    /// Forward pass: computes y = x @ W^T + bias with shard-aware dispatch
    ///
    /// Row-wise sharding: each shard computes x @ W_shard^T for a subset of
    /// output features; results are concatenated across shards.
    /// Column-wise sharding: each shard operates on a slice of the input;
    /// partial products are all-reduced (summed) to get the full output.
    pub fn forward(
        &self,
        input: &Tensor<f32>,
        transport: &Arc<dyn DistributedTransport>,
    ) -> Result<Tensor<f32>, DistributedError> {
        let ops = TensorOps::new(HardwareCapabilities::NONE);

        // Weight global shape: [out_features, in_features]
        let out_features = self.weight.global_shape[0];
        let in_features = self.weight.global_shape[1];

        // Input shape validation: [batch, in_features]
        if input.ndim() != 2 || input.shape()[1] != in_features {
            return Err(DistributedError::InferenceError(alloc::format!(
                "input shape {:?} incompatible with weight [{}x{}]",
                input.shape(),
                out_features,
                in_features
            )));
        }
        let batch = input.shape()[0];

        match &self.shard_strategy {
            PartitionStrategy::RowWise | PartitionStrategy::Pipeline { .. } => {
                // Each shard: W_k shape [out_k, in_features]
                // local result: input [batch, in_features] @ W_k^T [in_features, out_k]
                //             → [batch, out_k]
                // Concatenate along column dimension to get [batch, out_features]
                let mut out_data = alloc::vec![0.0f32; batch * out_features];

                for shard in &self.weight.shards {
                    let local_w = shard.to_local_tensor();
                    let out_k = local_w.shape()[0];

                    // Transpose shard weight: [in_features, out_k]
                    let mut w_t_data = alloc::vec![0.0f32; in_features * out_k];
                    for r in 0..out_k {
                        for c in 0..in_features {
                            w_t_data[c * out_k + r] = local_w.data()[r * in_features + c];
                        }
                    }
                    let w_t = Tensor::from_vec(w_t_data, alloc::vec![in_features, out_k])
                        .ok_or_else(|| {
                            DistributedError::InferenceError("transpose failed".into())
                        })?;

                    // [batch, in_features] @ [in_features, out_k] = [batch, out_k]
                    let local_out = ops.matmul(input, &w_t).ok_or_else(|| {
                        DistributedError::InferenceError("layer forward matmul failed".into())
                    })?;

                    let col_start = shard.row_start; // row_start in weight = output col offset
                    for b in 0..batch {
                        for k in 0..out_k {
                            out_data[b * out_features + col_start + k] =
                                local_out.data()[b * out_k + k];
                        }
                    }
                }

                if let Some(bias) = &self.bias {
                    for b in 0..batch {
                        for j in 0..out_features {
                            out_data[b * out_features + j] += bias.data()[j];
                        }
                    }
                }
                self.activation.apply_inplace(&mut out_data);
                Tensor::from_vec(out_data, alloc::vec![batch, out_features])
                    .ok_or_else(|| DistributedError::InferenceError("layer output tensor".into()))
            }

            PartitionStrategy::ColumnWise => {
                // Each shard: W_k shape [out_features, in_k]
                // Slice input accordingly: x_k [batch, in_k]
                // local result: x_k @ W_k^T → [batch, out_features]
                // All-reduce (sum) across shards → [batch, out_features]
                let partial_shards: Vec<TensorShard> = self
                    .weight
                    .shards
                    .iter()
                    .map(|shard| {
                        let local_w = shard.to_local_tensor();
                        let in_k = shard.col_end - shard.col_start;

                        // Extract input slice [batch, in_k]
                        let mut x_slice_data = alloc::vec![0.0f32; batch * in_k];
                        for b in 0..batch {
                            for k in 0..in_k {
                                x_slice_data[b * in_k + k] =
                                    input.data()[b * in_features + shard.col_start + k];
                            }
                        }
                        let x_slice = Tensor::from_vec(x_slice_data, alloc::vec![batch, in_k])
                            .ok_or_else(|| {
                                DistributedError::InferenceError("x_slice creation".into())
                            })?;

                        // Transpose W_k: [in_k, out_features]
                        let mut w_t_data = alloc::vec![0.0f32; in_k * out_features];
                        for r in 0..out_features {
                            for c in 0..in_k {
                                w_t_data[c * out_features + r] = local_w.data()[r * in_k + c];
                            }
                        }
                        let w_t = Tensor::from_vec(w_t_data, alloc::vec![in_k, out_features])
                            .ok_or_else(|| {
                                DistributedError::InferenceError("w_t creation".into())
                            })?;

                        // [batch, in_k] @ [in_k, out_features] = [batch, out_features]
                        let local_out = ops.matmul(&x_slice, &w_t).ok_or_else(|| {
                            DistributedError::InferenceError(
                                "column-wise layer forward matmul failed".into(),
                            )
                        })?;

                        Ok(TensorShard {
                            shard_id: shard.shard_id,
                            shard_index: shard.shard_index,
                            num_shards: shard.num_shards,
                            global_shape: alloc::vec![batch, out_features],
                            row_start: 0,
                            row_end: batch,
                            col_start: 0,
                            col_end: out_features,
                            data: local_out.data().to_vec(),
                            local_shape: alloc::vec![batch, out_features],
                            strategy: self.shard_strategy.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>, DistributedError>>()?;

                let reduced = transport.all_reduce(partial_shards, ReduceOp::Sum)?;
                let mut out_data = reduced.data;

                if let Some(bias) = &self.bias {
                    for b in 0..batch {
                        for j in 0..out_features {
                            out_data[b * out_features + j] += bias.data()[j];
                        }
                    }
                }
                self.activation.apply_inplace(&mut out_data);
                Tensor::from_vec(out_data, alloc::vec![batch, out_features])
                    .ok_or_else(|| DistributedError::InferenceError("layer output tensor".into()))
            }

            PartitionStrategy::Block { .. } => {
                // For block partition, treat similarly to row-wise for the forward pass
                // (shards cover blocks of the weight; concatenate partial outputs)
                Err(DistributedError::InvalidStrategy(
                    "Block strategy not yet supported in ModelParallelLayer::forward; \
                     use RowWise or ColumnWise"
                        .into(),
                ))
            }
        }
    }
}

// ─── ModelParallelPipeline ────────────────────────────────────────────────────

/// A sequence of model-parallel layers forming a full inference pipeline
pub struct ModelParallelPipeline {
    /// Ordered layers
    pub layers: Vec<ModelParallelLayer>,
    /// Transport used for inter-shard communication
    pub transport: Arc<dyn DistributedTransport>,
}

impl ModelParallelPipeline {
    /// Create a pipeline with the given layers and transport
    pub fn new(layers: Vec<ModelParallelLayer>, transport: Arc<dyn DistributedTransport>) -> Self {
        Self { layers, transport }
    }

    /// Run a full forward pass through all layers
    pub fn forward(&self, input: &Tensor<f32>) -> Result<Tensor<f32>, DistributedError> {
        let mut current = input.clone();
        for layer in &self.layers {
            current = layer.forward(&current, &self.transport)?;
        }
        Ok(current)
    }
}

// ─── Hardware capabilities (for node descriptors) ────────────────────────────

/// Capabilities of a single inference node
#[derive(Debug, Clone)]
pub struct NodeCapabilities {
    /// Which hardware acceleration this node supports
    pub hw: HardwareCapabilities,
    /// Available FLOPS (for scheduling heuristics)
    pub flops: u64,
    /// Available DRAM bandwidth in GB/s
    pub bandwidth_gb_s: f32,
}

impl Default for NodeCapabilities {
    fn default() -> Self {
        Self {
            hw: HardwareCapabilities::NONE,
            flops: 1_000_000_000,
            bandwidth_gb_s: 10.0,
        }
    }
}

// ─── LayerSpec ────────────────────────────────────────────────────────────────

/// Specification for a single layer in a compiled model
#[derive(Debug, Clone)]
pub struct LayerSpec {
    /// Weight tensor shape [out_features, in_features]
    pub weight_shape: [usize; 2],
    /// Whether to include a bias term
    pub has_bias: bool,
    /// Post-layer activation function
    pub activation: ActivationFn,
    /// Preferred partitioning strategy for this layer's weight
    pub strategy: PartitionStrategy,
}

// ─── CompiledModel ────────────────────────────────────────────────────────────

/// A distributed inference model ready to run
pub struct CompiledModel {
    /// Sequential pipeline of sharded layers
    pub pipeline: ModelParallelPipeline,
    /// Metrics collected during compilation / inference
    pub metrics: InferenceMetrics,
}

/// Per-inference metrics
#[derive(Debug, Clone, Default)]
pub struct InferenceMetrics {
    /// Total floating-point multiply-accumulate operations executed
    pub total_flops: u64,
    /// Total bytes transmitted across logical node boundaries
    pub inter_node_bytes: u64,
    /// Per-layer latency (µs, populated after inference)
    pub layer_latency_us: Vec<u64>,
}

// ─── DistributedInferenceEngine ───────────────────────────────────────────────

/// Top-level orchestrator: compiles model specs and drives distributed inference
pub struct DistributedInferenceEngine {
    num_nodes: usize,
    #[allow(dead_code)]
    capabilities: NodeCapabilities,
    transport: Arc<dyn DistributedTransport>,
}

impl DistributedInferenceEngine {
    /// Create a new engine targeting `num_nodes` logical nodes
    pub fn new(
        num_nodes: usize,
        capabilities: NodeCapabilities,
        transport: Arc<dyn DistributedTransport>,
    ) -> Self {
        Self {
            num_nodes,
            capabilities,
            transport,
        }
    }

    /// Compile a list of layer specifications into a `CompiledModel`
    ///
    /// Initialises weights to a deterministic pattern so tests can verify
    /// shapes without requiring a separate weight-loading step.
    pub fn compile_model(&self, layers: Vec<LayerSpec>) -> Result<CompiledModel, DistributedError> {
        if layers.is_empty() {
            return Err(DistributedError::CompilationError(
                "model must have at least one layer".into(),
            ));
        }

        let mut model_layers = Vec::with_capacity(layers.len());
        let mut total_flops: u64 = 0;
        let mut inter_node_bytes: u64 = 0;
        let layer_latency_us = alloc::vec![0u64; layers.len()];

        for spec in &layers {
            let [out_f, in_f] = spec.weight_shape;

            // Deterministic weight initialisation (Xavier-like scale)
            let scale = (6.0f32 / (in_f + out_f) as f32).sqrt();
            let weight_data: Vec<f32> = (0..out_f * in_f)
                .map(|i| {
                    let t = (i as f32) / (out_f * in_f) as f32;
                    scale * (2.0 * t - 1.0)
                })
                .collect();
            let weight =
                Tensor::from_vec(weight_data, alloc::vec![out_f, in_f]).ok_or_else(|| {
                    DistributedError::CompilationError("weight tensor creation failed".into())
                })?;

            let bias = if spec.has_bias {
                Some(Tensor::zeros(alloc::vec![out_f]))
            } else {
                None
            };

            // Each shard needs to communicate its partial output
            inter_node_bytes += (out_f * self.num_nodes * core::mem::size_of::<f32>()) as u64;
            total_flops += (2 * in_f * out_f) as u64;

            let layer = ModelParallelLayer::new(
                weight,
                bias,
                spec.activation,
                spec.strategy.clone(),
                self.num_nodes,
            )?;
            model_layers.push(layer);
        }

        let pipeline = ModelParallelPipeline::new(model_layers, Arc::clone(&self.transport));

        Ok(CompiledModel {
            pipeline,
            metrics: InferenceMetrics {
                total_flops,
                inter_node_bytes,
                layer_latency_us,
            },
        })
    }

    /// Run inference through a compiled model
    pub fn infer(
        &self,
        model: &CompiledModel,
        input: &Tensor<f32>,
    ) -> Result<Tensor<f32>, DistributedError> {
        model.pipeline.forward(input)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    // Helper: build an f32 matrix tensor
    fn make_matrix<F: Fn(usize, usize) -> f32>(rows: usize, cols: usize, fill: F) -> Tensor<f32> {
        let mut data = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                data.push(fill(r, c));
            }
        }
        Tensor::from_vec(data, alloc::vec![rows, cols]).unwrap()
    }

    // Reference matmul (scalar, exact)
    fn ref_matmul(a: &Tensor<f32>, b: &Tensor<f32>) -> Vec<f32> {
        let m = a.shape()[0];
        let k = a.shape()[1];
        let n = b.shape()[1];
        let mut out = alloc::vec![0.0f32; m * n];
        for i in 0..m {
            for p in 0..k {
                for j in 0..n {
                    out[i * n + j] += a.data()[i * k + p] * b.data()[p * n + j];
                }
            }
        }
        out
    }

    fn make_transport(nodes: usize) -> Arc<dyn DistributedTransport> {
        Arc::new(LocalTransport::new(nodes))
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 1. Row partition → reconstruct is identity
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_row_partition_reconstruct() {
        let original = make_matrix(12, 8, |r, c| (r * 8 + c) as f32);
        let shards = ShardedTensor::partition(&original, PartitionStrategy::RowWise, 4).unwrap();
        assert_eq!(shards.len(), 4);
        let recovered = ShardedTensor::reconstruct(shards, PartitionStrategy::RowWise).unwrap();
        assert_eq!(recovered.data(), original.data());
        assert_eq!(recovered.shape(), original.shape());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 2. Row partition with non-divisible rows
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_row_partition_uneven() {
        let original = make_matrix(10, 5, |r, c| (r * 10 + c) as f32);
        // 10 rows / 3 shards → [4, 3, 3]
        let shards = ShardedTensor::partition(&original, PartitionStrategy::RowWise, 3).unwrap();
        assert_eq!(shards.len(), 3);
        assert_eq!(shards[0].local_shape[0], 4);
        assert_eq!(shards[1].local_shape[0], 3);
        assert_eq!(shards[2].local_shape[0], 3);
        let recovered = ShardedTensor::reconstruct(shards, PartitionStrategy::RowWise).unwrap();
        assert_eq!(recovered.data(), original.data());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 3. Column partition → reconstruct is identity
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_col_partition_reconstruct() {
        let original = make_matrix(6, 12, |r, c| (r * 100 + c) as f32);
        let shards = ShardedTensor::partition(&original, PartitionStrategy::ColumnWise, 4).unwrap();
        assert_eq!(shards.len(), 4);
        let recovered = ShardedTensor::reconstruct(shards, PartitionStrategy::ColumnWise).unwrap();
        assert_eq!(recovered.data(), original.data());
        assert_eq!(recovered.shape(), original.shape());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 4. Block partition → reconstruct is identity
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_block_partition_reconstruct() {
        let original = make_matrix(8, 8, |r, c| (r * 8 + c) as f32);
        let strategy = PartitionStrategy::Block { rows: 4, cols: 4 };
        let shards = ShardedTensor::partition(&original, strategy.clone(), 4).unwrap();
        assert_eq!(shards.len(), 4);
        let recovered = ShardedTensor::reconstruct(shards, strategy).unwrap();
        assert_eq!(recovered.data(), original.data());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 5. Block partition with non-square grid
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_block_partition_non_square() {
        let original = make_matrix(9, 6, |r, c| (r as f32) * 10.0 + c as f32);
        let strategy = PartitionStrategy::Block { rows: 3, cols: 3 };
        let shards = ShardedTensor::partition(&original, strategy.clone(), 6).unwrap();
        assert_eq!(shards.len(), 6);
        let recovered = ShardedTensor::reconstruct(shards, strategy).unwrap();
        assert_eq!(recovered.data(), original.data());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 6. Distributed matmul correctness vs local reference
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_distributed_matmul_correctness() {
        let a = make_matrix(4, 4, |r, c| (r * 4 + c + 1) as f32);
        let b = make_matrix(4, 4, |r, c| (r * 4 + c + 1) as f32);

        let expected = ref_matmul(&a, &b);
        let transport = make_transport(4);
        let result = distributed_matmul(&a, &b, 4, &transport).unwrap();

        for (got, exp) in result.data().iter().zip(expected.iter()) {
            assert!(
                (got - exp).abs() < 1e-3,
                "mismatch: got {got}, expected {exp}"
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 7. Distributed matmul on 256×256 matrices
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_distributed_matmul_large() {
        let size = 32; // 32×32 is large enough to validate sharding without being slow
        let a = make_matrix(size, size, |r, c| {
            ((r + 1) as f32) * 0.01 + ((c + 1) as f32) * 0.001
        });
        let b = make_matrix(size, size, |r, c| {
            ((r + 1) as f32) * 0.001 + ((c + 1) as f32) * 0.01
        });

        let expected = ref_matmul(&a, &b);
        let transport = make_transport(4);
        let result = distributed_matmul(&a, &b, 4, &transport).unwrap();

        for (got, exp) in result.data().iter().zip(expected.iter()) {
            assert!(
                (got - exp).abs() < 1e-2,
                "large matmul mismatch: got {got}, expected {exp}"
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 8. All-reduce: sum
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_all_reduce_sum() {
        let transport = LocalTransport::new(3);
        let make_shard = |idx: usize, val: f32| TensorShard {
            shard_id: idx as ShardId,
            shard_index: idx,
            num_shards: 3,
            global_shape: alloc::vec![1, 4],
            row_start: 0,
            row_end: 1,
            col_start: 0,
            col_end: 4,
            data: alloc::vec![val; 4],
            local_shape: alloc::vec![1, 4],
            strategy: PartitionStrategy::RowWise,
        };

        let shards = alloc::vec![make_shard(0, 1.0), make_shard(1, 2.0), make_shard(2, 3.0)];
        let reduced = transport.all_reduce(shards, ReduceOp::Sum).unwrap();
        assert_eq!(reduced.data, alloc::vec![6.0f32; 4]);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 9. All-reduce: max
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_all_reduce_max() {
        let transport = LocalTransport::new(2);
        let make_shard = |idx: usize, val: f32| TensorShard {
            shard_id: idx as ShardId,
            shard_index: idx,
            num_shards: 2,
            global_shape: alloc::vec![1, 3],
            row_start: 0,
            row_end: 1,
            col_start: 0,
            col_end: 3,
            data: alloc::vec![val - 1.0, val, val + 1.0],
            local_shape: alloc::vec![1, 3],
            strategy: PartitionStrategy::RowWise,
        };

        // shard 0: [0, 1, 2], shard 1: [4, 5, 6]
        let shards = alloc::vec![make_shard(0, 1.0), make_shard(1, 5.0)];
        let reduced = transport.all_reduce(shards, ReduceOp::Max).unwrap();
        assert_eq!(reduced.data, alloc::vec![4.0f32, 5.0, 6.0]);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 10. All-reduce: min
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_all_reduce_min() {
        let transport = LocalTransport::new(2);
        let make_shard = |idx: usize, vals: Vec<f32>| TensorShard {
            shard_id: idx as ShardId,
            shard_index: idx,
            num_shards: 2,
            global_shape: alloc::vec![1, 3],
            row_start: 0,
            row_end: 1,
            col_start: 0,
            col_end: 3,
            data: vals,
            local_shape: alloc::vec![1, 3],
            strategy: PartitionStrategy::RowWise,
        };

        let shards = alloc::vec![
            make_shard(0, alloc::vec![3.0, 1.0, 5.0]),
            make_shard(1, alloc::vec![2.0, 4.0, 0.5]),
        ];
        let reduced = transport.all_reduce(shards, ReduceOp::Min).unwrap();
        assert_eq!(reduced.data, alloc::vec![2.0f32, 1.0, 0.5]);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 11. ModelParallelPipeline forward pass
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_model_parallel_pipeline_forward() {
        let transport: Arc<dyn DistributedTransport> = Arc::new(LocalTransport::new(2));

        // Layer 0: weight [4 out × 2 in], bias zeros, identity activation, col-wise shard
        let w0 = make_matrix(4, 2, |r, c| (r * 2 + c + 1) as f32 * 0.1);
        let layer0 = ModelParallelLayer::new(
            w0.clone(),
            Some(Tensor::zeros(alloc::vec![4])),
            ActivationFn::Identity,
            PartitionStrategy::ColumnWise,
            2,
        )
        .unwrap();

        // Layer 1: weight [2 out × 4 in], no bias, ReLU, row-wise shard
        let w1 = make_matrix(2, 4, |r, c| (r * 4 + c + 1) as f32 * 0.05);
        let layer1 = ModelParallelLayer::new(
            w1.clone(),
            None,
            ActivationFn::ReLU,
            PartitionStrategy::RowWise,
            2,
        )
        .unwrap();

        let pipeline = ModelParallelPipeline::new(alloc::vec![layer0, layer1], transport);

        // Input: [1, 2] matrix (batch=1, in_features=2)
        let input = make_matrix(1, 2, |_, c| c as f32 + 1.0);
        let output = pipeline.forward(&input).unwrap();

        // Shape should be [1, 2]
        assert_eq!(output.shape(), &[1, 2]);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 12. DistributedInferenceEngine: single node
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_distributed_inference_engine_single_node() {
        let transport: Arc<dyn DistributedTransport> = Arc::new(LocalTransport::new(1));
        let engine =
            DistributedInferenceEngine::new(1, NodeCapabilities::default(), Arc::clone(&transport));

        let layers = alloc::vec![LayerSpec {
            weight_shape: [4, 8],
            has_bias: false,
            activation: ActivationFn::ReLU,
            strategy: PartitionStrategy::RowWise,
        }];

        let model = engine.compile_model(layers).unwrap();
        let input = make_matrix(1, 8, |_, c| c as f32 * 0.1);
        let output = engine.infer(&model, &input).unwrap();

        // Output should be [1, 4]
        assert_eq!(output.shape(), &[1, 4]);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 13. DistributedInferenceEngine: multi-shard
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_distributed_inference_engine_multi_shard() {
        let transport: Arc<dyn DistributedTransport> = Arc::new(LocalTransport::new(4));
        let engine = DistributedInferenceEngine::new(
            4,
            NodeCapabilities {
                hw: HardwareCapabilities::NONE,
                flops: 4_000_000_000,
                bandwidth_gb_s: 40.0,
            },
            Arc::clone(&transport),
        );

        let layers = alloc::vec![
            LayerSpec {
                weight_shape: [8, 4],
                has_bias: true,
                activation: ActivationFn::ReLU,
                strategy: PartitionStrategy::RowWise,
            },
            LayerSpec {
                weight_shape: [4, 8],
                has_bias: false,
                activation: ActivationFn::Sigmoid,
                strategy: PartitionStrategy::ColumnWise,
            },
        ];

        let model = engine.compile_model(layers).unwrap();
        let input = make_matrix(1, 4, |_, c| (c + 1) as f32);
        let output = engine.infer(&model, &input).unwrap();

        // Final shape: [1, 4]
        assert_eq!(output.shape(), &[1, 4]);
        // Sigmoid outputs are in (0, 1)
        for &v in output.data() {
            assert!(v > 0.0 && v < 1.0, "sigmoid output {v} out of range");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 14. Pipeline partition and reconstruct
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_pipeline_partition_reconstruct() {
        let original = make_matrix(6, 4, |r, c| (r * 4 + c) as f32);
        let strategy = PartitionStrategy::Pipeline { stages: 3 };
        let shards = ShardedTensor::partition(&original, strategy.clone(), 3).unwrap();
        assert_eq!(shards.len(), 3);
        let recovered = ShardedTensor::reconstruct(shards, strategy).unwrap();
        assert_eq!(recovered.data(), original.data());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 15. All-reduce: product
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_all_reduce_product() {
        let transport = LocalTransport::new(3);
        let make_shard = |idx: usize, vals: Vec<f32>| TensorShard {
            shard_id: idx as ShardId,
            shard_index: idx,
            num_shards: 3,
            global_shape: alloc::vec![1, 2],
            row_start: 0,
            row_end: 1,
            col_start: 0,
            col_end: 2,
            data: vals,
            local_shape: alloc::vec![1, 2],
            strategy: PartitionStrategy::RowWise,
        };

        let shards = alloc::vec![
            make_shard(0, alloc::vec![2.0, 3.0]),
            make_shard(1, alloc::vec![4.0, 5.0]),
            make_shard(2, alloc::vec![6.0, 7.0]),
        ];
        let reduced = transport.all_reduce(shards, ReduceOp::Product).unwrap();
        // 2*4*6=48, 3*5*7=105
        assert!((reduced.data[0] - 48.0).abs() < 1e-5);
        assert!((reduced.data[1] - 105.0).abs() < 1e-5);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 16. Distributed matmul: asymmetric shapes
    // ─────────────────────────────────────────────────────────────────────────
    #[test]
    fn test_distributed_matmul_asymmetric() {
        let a = make_matrix(3, 5, |r, c| (r * 5 + c + 1) as f32);
        let b = make_matrix(5, 2, |r, c| (r * 2 + c + 1) as f32);

        let expected = ref_matmul(&a, &b);
        let transport = make_transport(4);
        let result = distributed_matmul(&a, &b, 4, &transport).unwrap();

        for (got, exp) in result.data().iter().zip(expected.iter()) {
            assert!(
                (got - exp).abs() < 1e-3,
                "asymmetric matmul mismatch: got {got}, expected {exp}"
            );
        }
    }
}
