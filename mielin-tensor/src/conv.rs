//! Convolution operations for neural networks
//!
//! Provides 1D and 2D convolution, depthwise convolution, and pooling operations.

extern crate alloc;
use crate::tensor::Tensor;

/// Convolution error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvError {
    /// Invalid input dimensions
    InvalidInputDimensions,
    /// Invalid kernel dimensions
    InvalidKernelDimensions,
    /// Stride must be positive
    InvalidStride,
    /// Padding mode not supported
    InvalidPadding,
    /// Input and kernel channel mismatch
    ChannelMismatch,
}

/// Padding mode for convolution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaddingMode {
    /// No padding - output is smaller
    Valid,
    /// Zero padding to keep output same size as input
    Same,
    /// Custom padding amounts
    Custom(usize),
}

/// Pooling mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolingMode {
    Max,
    Average,
}

/// Convolution operations
pub struct ConvOps;

impl ConvOps {
    /// 1D convolution
    ///
    /// Input shape: `[length]`
    /// Kernel shape: `[kernel_size]`
    /// Output shape: `[output_length]` where output_length depends on padding/stride
    pub fn conv1d(
        input: &Tensor<f32>,
        kernel: &Tensor<f32>,
        stride: usize,
        padding: PaddingMode,
    ) -> Result<Tensor<f32>, ConvError> {
        if input.ndim() != 1 || kernel.ndim() != 1 {
            return Err(ConvError::InvalidInputDimensions);
        }
        if stride == 0 {
            return Err(ConvError::InvalidStride);
        }

        let input_len = input.shape()[0];
        let kernel_len = kernel.shape()[0];

        let pad = match padding {
            PaddingMode::Valid => 0,
            PaddingMode::Same => (kernel_len - 1) / 2,
            PaddingMode::Custom(p) => p,
        };

        // Calculate output size
        let output_len = (input_len + 2 * pad - kernel_len) / stride + 1;

        let mut output_data = alloc::vec![0.0f32; output_len];

        let input_data = input.data();
        let kernel_data = kernel.data();

        for (i, out_val) in output_data.iter_mut().enumerate() {
            let start = i * stride;
            let mut sum = 0.0f32;

            for (k, &kernel_val) in kernel_data.iter().enumerate() {
                let input_idx = start as isize + k as isize - pad as isize;
                if input_idx >= 0 && input_idx < input_len as isize {
                    sum += input_data[input_idx as usize] * kernel_val;
                }
                // If out of bounds, treat as zero padding
            }

            *out_val = sum;
        }

        Ok(Tensor::from_vec(output_data, alloc::vec![output_len])
            .expect("output_data length matches output_len"))
    }

    /// 2D convolution (for single channel)
    ///
    /// Input shape: [height, width]
    /// Kernel shape: [kernel_height, kernel_width]
    /// Output shape: [output_height, output_width]
    pub fn conv2d(
        input: &Tensor<f32>,
        kernel: &Tensor<f32>,
        stride: (usize, usize),
        padding: PaddingMode,
    ) -> Result<Tensor<f32>, ConvError> {
        if input.ndim() != 2 || kernel.ndim() != 2 {
            return Err(ConvError::InvalidInputDimensions);
        }
        if stride.0 == 0 || stride.1 == 0 {
            return Err(ConvError::InvalidStride);
        }

        let input_h = input.shape()[0];
        let input_w = input.shape()[1];
        let kernel_h = kernel.shape()[0];
        let kernel_w = kernel.shape()[1];

        let (pad_h, pad_w) = match padding {
            PaddingMode::Valid => (0, 0),
            PaddingMode::Same => ((kernel_h - 1) / 2, (kernel_w - 1) / 2),
            PaddingMode::Custom(p) => (p, p),
        };

        // Calculate output size
        let output_h = (input_h + 2 * pad_h - kernel_h) / stride.0 + 1;
        let output_w = (input_w + 2 * pad_w - kernel_w) / stride.1 + 1;

        let mut output = Tensor::zeros(alloc::vec![output_h, output_w]);

        for oh in 0..output_h {
            for ow in 0..output_w {
                let start_h = oh * stride.0;
                let start_w = ow * stride.1;
                let mut sum = 0.0f32;

                for kh in 0..kernel_h {
                    for kw in 0..kernel_w {
                        let ih = start_h as isize + kh as isize - pad_h as isize;
                        let iw = start_w as isize + kw as isize - pad_w as isize;

                        if ih >= 0 && ih < input_h as isize && iw >= 0 && iw < input_w as isize {
                            let input_val = input
                                .get(&[ih as usize, iw as usize])
                                .expect("ih,iw bounds-checked above");
                            let kernel_val = kernel
                                .get(&[kh, kw])
                                .expect("kh < kernel_h and kw < kernel_w");
                            sum += input_val * kernel_val;
                        }
                    }
                }

                output.set(&[oh, ow], sum);
            }
        }

        Ok(output)
    }

    /// 2D convolution with batches and channels
    ///
    /// Input shape: [batch, in_channels, height, width]
    /// Kernel shape: [out_channels, in_channels, kernel_height, kernel_width]
    /// Output shape: [batch, out_channels, output_height, output_width]
    pub fn conv2d_batch(
        input: &Tensor<f32>,
        kernel: &Tensor<f32>,
        stride: (usize, usize),
        padding: PaddingMode,
    ) -> Result<Tensor<f32>, ConvError> {
        if input.ndim() != 4 || kernel.ndim() != 4 {
            return Err(ConvError::InvalidInputDimensions);
        }
        if stride.0 == 0 || stride.1 == 0 {
            return Err(ConvError::InvalidStride);
        }

        let batch_size = input.shape()[0];
        let in_channels = input.shape()[1];
        let input_h = input.shape()[2];
        let input_w = input.shape()[3];

        let out_channels = kernel.shape()[0];
        let kernel_in_ch = kernel.shape()[1];
        let kernel_h = kernel.shape()[2];
        let kernel_w = kernel.shape()[3];

        if in_channels != kernel_in_ch {
            return Err(ConvError::ChannelMismatch);
        }

        let (pad_h, pad_w) = match padding {
            PaddingMode::Valid => (0, 0),
            PaddingMode::Same => ((kernel_h - 1) / 2, (kernel_w - 1) / 2),
            PaddingMode::Custom(p) => (p, p),
        };

        let output_h = (input_h + 2 * pad_h - kernel_h) / stride.0 + 1;
        let output_w = (input_w + 2 * pad_w - kernel_w) / stride.1 + 1;

        let output_size = batch_size * out_channels * output_h * output_w;
        let mut output_data = alloc::vec![0.0f32; output_size];

        let input_data = input.data();
        let kernel_data = kernel.data();

        // Compute strides for indexing
        let input_batch_stride = in_channels * input_h * input_w;
        let input_channel_stride = input_h * input_w;
        let kernel_out_stride = kernel_in_ch * kernel_h * kernel_w;
        let kernel_in_stride = kernel_h * kernel_w;
        let output_batch_stride = out_channels * output_h * output_w;
        let output_channel_stride = output_h * output_w;

        for b in 0..batch_size {
            for oc in 0..out_channels {
                for oh in 0..output_h {
                    for ow in 0..output_w {
                        let start_h = oh * stride.0;
                        let start_w = ow * stride.1;
                        let mut sum = 0.0f32;

                        for ic in 0..in_channels {
                            for kh in 0..kernel_h {
                                for kw in 0..kernel_w {
                                    let ih = start_h as isize + kh as isize - pad_h as isize;
                                    let iw = start_w as isize + kw as isize - pad_w as isize;

                                    if ih >= 0
                                        && ih < input_h as isize
                                        && iw >= 0
                                        && iw < input_w as isize
                                    {
                                        let input_idx = b * input_batch_stride
                                            + ic * input_channel_stride
                                            + (ih as usize) * input_w
                                            + (iw as usize);
                                        let kernel_idx = oc * kernel_out_stride
                                            + ic * kernel_in_stride
                                            + kh * kernel_w
                                            + kw;

                                        sum += input_data[input_idx] * kernel_data[kernel_idx];
                                    }
                                }
                            }
                        }

                        let output_idx = b * output_batch_stride
                            + oc * output_channel_stride
                            + oh * output_w
                            + ow;
                        output_data[output_idx] = sum;
                    }
                }
            }
        }

        Tensor::from_vec(
            output_data,
            alloc::vec![batch_size, out_channels, output_h, output_w],
        )
        .ok_or(ConvError::InvalidInputDimensions)
    }

    /// Depthwise 2D convolution
    ///
    /// Each input channel is convolved separately with its own kernel.
    /// Input shape: [batch, channels, height, width]
    /// Kernel shape: [channels, kernel_height, kernel_width]
    /// Output shape: [batch, channels, output_height, output_width]
    pub fn depthwise_conv2d(
        input: &Tensor<f32>,
        kernel: &Tensor<f32>,
        stride: (usize, usize),
        padding: PaddingMode,
    ) -> Result<Tensor<f32>, ConvError> {
        if input.ndim() != 4 || kernel.ndim() != 3 {
            return Err(ConvError::InvalidInputDimensions);
        }
        if stride.0 == 0 || stride.1 == 0 {
            return Err(ConvError::InvalidStride);
        }

        let batch_size = input.shape()[0];
        let channels = input.shape()[1];
        let input_h = input.shape()[2];
        let input_w = input.shape()[3];

        let kernel_ch = kernel.shape()[0];
        let kernel_h = kernel.shape()[1];
        let kernel_w = kernel.shape()[2];

        if channels != kernel_ch {
            return Err(ConvError::ChannelMismatch);
        }

        let (pad_h, pad_w) = match padding {
            PaddingMode::Valid => (0, 0),
            PaddingMode::Same => ((kernel_h - 1) / 2, (kernel_w - 1) / 2),
            PaddingMode::Custom(p) => (p, p),
        };

        let output_h = (input_h + 2 * pad_h - kernel_h) / stride.0 + 1;
        let output_w = (input_w + 2 * pad_w - kernel_w) / stride.1 + 1;

        let output_size = batch_size * channels * output_h * output_w;
        let mut output_data = alloc::vec![0.0f32; output_size];

        let input_data = input.data();
        let kernel_data = kernel.data();

        let input_batch_stride = channels * input_h * input_w;
        let input_channel_stride = input_h * input_w;
        let kernel_channel_stride = kernel_h * kernel_w;
        let output_batch_stride = channels * output_h * output_w;
        let output_channel_stride = output_h * output_w;

        for b in 0..batch_size {
            for c in 0..channels {
                for oh in 0..output_h {
                    for ow in 0..output_w {
                        let start_h = oh * stride.0;
                        let start_w = ow * stride.1;
                        let mut sum = 0.0f32;

                        for kh in 0..kernel_h {
                            for kw in 0..kernel_w {
                                let ih = start_h as isize + kh as isize - pad_h as isize;
                                let iw = start_w as isize + kw as isize - pad_w as isize;

                                if ih >= 0
                                    && ih < input_h as isize
                                    && iw >= 0
                                    && iw < input_w as isize
                                {
                                    let input_idx = b * input_batch_stride
                                        + c * input_channel_stride
                                        + (ih as usize) * input_w
                                        + (iw as usize);
                                    let kernel_idx = c * kernel_channel_stride + kh * kernel_w + kw;

                                    sum += input_data[input_idx] * kernel_data[kernel_idx];
                                }
                            }
                        }

                        let output_idx = b * output_batch_stride
                            + c * output_channel_stride
                            + oh * output_w
                            + ow;
                        output_data[output_idx] = sum;
                    }
                }
            }
        }

        Tensor::from_vec(
            output_data,
            alloc::vec![batch_size, channels, output_h, output_w],
        )
        .ok_or(ConvError::InvalidInputDimensions)
    }

    /// 2D max pooling
    ///
    /// Input shape: [height, width]
    /// Output shape: [output_height, output_width]
    pub fn max_pool2d(
        input: &Tensor<f32>,
        pool_size: (usize, usize),
        stride: (usize, usize),
    ) -> Result<Tensor<f32>, ConvError> {
        Self::pool2d_impl(input, pool_size, stride, PoolingMode::Max)
    }

    /// 2D average pooling
    ///
    /// Input shape: [height, width]
    /// Output shape: [output_height, output_width]
    pub fn avg_pool2d(
        input: &Tensor<f32>,
        pool_size: (usize, usize),
        stride: (usize, usize),
    ) -> Result<Tensor<f32>, ConvError> {
        Self::pool2d_impl(input, pool_size, stride, PoolingMode::Average)
    }

    fn pool2d_impl(
        input: &Tensor<f32>,
        pool_size: (usize, usize),
        stride: (usize, usize),
        mode: PoolingMode,
    ) -> Result<Tensor<f32>, ConvError> {
        if input.ndim() != 2 {
            return Err(ConvError::InvalidInputDimensions);
        }
        if stride.0 == 0 || stride.1 == 0 || pool_size.0 == 0 || pool_size.1 == 0 {
            return Err(ConvError::InvalidStride);
        }

        let input_h = input.shape()[0];
        let input_w = input.shape()[1];

        let output_h = (input_h - pool_size.0) / stride.0 + 1;
        let output_w = (input_w - pool_size.1) / stride.1 + 1;

        let mut output = Tensor::zeros(alloc::vec![output_h, output_w]);

        for oh in 0..output_h {
            for ow in 0..output_w {
                let start_h = oh * stride.0;
                let start_w = ow * stride.1;

                let value = match mode {
                    PoolingMode::Max => {
                        let mut max_val = f32::NEG_INFINITY;
                        for ph in 0..pool_size.0 {
                            for pw in 0..pool_size.1 {
                                let ih = start_h + ph;
                                let iw = start_w + pw;
                                if ih < input_h && iw < input_w {
                                    let v =
                                        *input.get(&[ih, iw]).expect("ih,iw bounds-checked above");
                                    if v > max_val {
                                        max_val = v;
                                    }
                                }
                            }
                        }
                        max_val
                    }
                    PoolingMode::Average => {
                        let mut sum = 0.0f32;
                        let mut count = 0;
                        for ph in 0..pool_size.0 {
                            for pw in 0..pool_size.1 {
                                let ih = start_h + ph;
                                let iw = start_w + pw;
                                if ih < input_h && iw < input_w {
                                    sum +=
                                        input.get(&[ih, iw]).expect("ih,iw bounds-checked above");
                                    count += 1;
                                }
                            }
                        }
                        if count > 0 {
                            sum / count as f32
                        } else {
                            0.0
                        }
                    }
                };

                output.set(&[oh, ow], value);
            }
        }

        Ok(output)
    }

    /// Batched 2D max pooling
    ///
    /// Input shape: [batch, channels, height, width]
    /// Output shape: [batch, channels, output_height, output_width]
    pub fn max_pool2d_batch(
        input: &Tensor<f32>,
        pool_size: (usize, usize),
        stride: (usize, usize),
    ) -> Result<Tensor<f32>, ConvError> {
        Self::pool2d_batch_impl(input, pool_size, stride, PoolingMode::Max)
    }

    /// Batched 2D average pooling
    ///
    /// Input shape: [batch, channels, height, width]
    /// Output shape: [batch, channels, output_height, output_width]
    pub fn avg_pool2d_batch(
        input: &Tensor<f32>,
        pool_size: (usize, usize),
        stride: (usize, usize),
    ) -> Result<Tensor<f32>, ConvError> {
        Self::pool2d_batch_impl(input, pool_size, stride, PoolingMode::Average)
    }

    fn pool2d_batch_impl(
        input: &Tensor<f32>,
        pool_size: (usize, usize),
        stride: (usize, usize),
        mode: PoolingMode,
    ) -> Result<Tensor<f32>, ConvError> {
        if input.ndim() != 4 {
            return Err(ConvError::InvalidInputDimensions);
        }
        if stride.0 == 0 || stride.1 == 0 || pool_size.0 == 0 || pool_size.1 == 0 {
            return Err(ConvError::InvalidStride);
        }

        let batch_size = input.shape()[0];
        let channels = input.shape()[1];
        let input_h = input.shape()[2];
        let input_w = input.shape()[3];

        let output_h = (input_h - pool_size.0) / stride.0 + 1;
        let output_w = (input_w - pool_size.1) / stride.1 + 1;

        let output_size = batch_size * channels * output_h * output_w;
        let mut output_data = alloc::vec![0.0f32; output_size];

        let input_data = input.data();
        let input_batch_stride = channels * input_h * input_w;
        let input_channel_stride = input_h * input_w;
        let output_batch_stride = channels * output_h * output_w;
        let output_channel_stride = output_h * output_w;

        for b in 0..batch_size {
            for c in 0..channels {
                for oh in 0..output_h {
                    for ow in 0..output_w {
                        let start_h = oh * stride.0;
                        let start_w = ow * stride.1;

                        let value = match mode {
                            PoolingMode::Max => {
                                let mut max_val = f32::NEG_INFINITY;
                                for ph in 0..pool_size.0 {
                                    for pw in 0..pool_size.1 {
                                        let ih = start_h + ph;
                                        let iw = start_w + pw;
                                        if ih < input_h && iw < input_w {
                                            let idx = b * input_batch_stride
                                                + c * input_channel_stride
                                                + ih * input_w
                                                + iw;
                                            if input_data[idx] > max_val {
                                                max_val = input_data[idx];
                                            }
                                        }
                                    }
                                }
                                max_val
                            }
                            PoolingMode::Average => {
                                let mut sum = 0.0f32;
                                let mut count = 0;
                                for ph in 0..pool_size.0 {
                                    for pw in 0..pool_size.1 {
                                        let ih = start_h + ph;
                                        let iw = start_w + pw;
                                        if ih < input_h && iw < input_w {
                                            let idx = b * input_batch_stride
                                                + c * input_channel_stride
                                                + ih * input_w
                                                + iw;
                                            sum += input_data[idx];
                                            count += 1;
                                        }
                                    }
                                }
                                if count > 0 {
                                    sum / count as f32
                                } else {
                                    0.0
                                }
                            }
                        };

                        let output_idx = b * output_batch_stride
                            + c * output_channel_stride
                            + oh * output_w
                            + ow;
                        output_data[output_idx] = value;
                    }
                }
            }
        }

        Tensor::from_vec(
            output_data,
            alloc::vec![batch_size, channels, output_h, output_w],
        )
        .ok_or(ConvError::InvalidInputDimensions)
    }

    /// Global average pooling - reduces spatial dimensions to 1x1
    ///
    /// Input shape: [batch, channels, height, width]
    /// Output shape: [batch, channels]
    pub fn global_avg_pool2d(input: &Tensor<f32>) -> Result<Tensor<f32>, ConvError> {
        if input.ndim() != 4 {
            return Err(ConvError::InvalidInputDimensions);
        }

        let batch_size = input.shape()[0];
        let channels = input.shape()[1];
        let height = input.shape()[2];
        let width = input.shape()[3];
        let spatial_size = height * width;

        let mut output_data = alloc::vec![0.0f32; batch_size * channels];
        let input_data = input.data();

        let input_batch_stride = channels * height * width;
        let input_channel_stride = height * width;

        for b in 0..batch_size {
            for c in 0..channels {
                let mut sum = 0.0f32;
                for h in 0..height {
                    for w in 0..width {
                        let idx = b * input_batch_stride + c * input_channel_stride + h * width + w;
                        sum += input_data[idx];
                    }
                }
                output_data[b * channels + c] = sum / spatial_size as f32;
            }
        }

        Tensor::from_vec(output_data, alloc::vec![batch_size, channels])
            .ok_or(ConvError::InvalidInputDimensions)
    }

    /// Global max pooling - reduces spatial dimensions to 1x1
    ///
    /// Input shape: [batch, channels, height, width]
    /// Output shape: [batch, channels]
    pub fn global_max_pool2d(input: &Tensor<f32>) -> Result<Tensor<f32>, ConvError> {
        if input.ndim() != 4 {
            return Err(ConvError::InvalidInputDimensions);
        }

        let batch_size = input.shape()[0];
        let channels = input.shape()[1];
        let height = input.shape()[2];
        let width = input.shape()[3];

        let mut output_data = alloc::vec![f32::NEG_INFINITY; batch_size * channels];
        let input_data = input.data();

        let input_batch_stride = channels * height * width;
        let input_channel_stride = height * width;

        for b in 0..batch_size {
            for c in 0..channels {
                let mut max_val = f32::NEG_INFINITY;
                for h in 0..height {
                    for w in 0..width {
                        let idx = b * input_batch_stride + c * input_channel_stride + h * width + w;
                        if input_data[idx] > max_val {
                            max_val = input_data[idx];
                        }
                    }
                }
                output_data[b * channels + c] = max_val;
            }
        }

        Tensor::from_vec(output_data, alloc::vec![batch_size, channels])
            .ok_or(ConvError::InvalidInputDimensions)
    }
}

/// Functional interface for convolution operations
pub mod functional {
    use super::*;

    /// 1D convolution with default stride and valid padding
    pub fn conv1d(input: &Tensor<f32>, kernel: &Tensor<f32>) -> Result<Tensor<f32>, ConvError> {
        ConvOps::conv1d(input, kernel, 1, PaddingMode::Valid)
    }

    /// 2D convolution with default stride and valid padding
    pub fn conv2d(input: &Tensor<f32>, kernel: &Tensor<f32>) -> Result<Tensor<f32>, ConvError> {
        ConvOps::conv2d(input, kernel, (1, 1), PaddingMode::Valid)
    }

    /// 2x2 max pooling with stride 2
    pub fn max_pool2d(input: &Tensor<f32>) -> Result<Tensor<f32>, ConvError> {
        ConvOps::max_pool2d(input, (2, 2), (2, 2))
    }

    /// 2x2 average pooling with stride 2
    pub fn avg_pool2d(input: &Tensor<f32>) -> Result<Tensor<f32>, ConvError> {
        ConvOps::avg_pool2d(input, (2, 2), (2, 2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn test_conv1d_valid() {
        let input = Tensor::vector(std::vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let kernel = Tensor::vector(std::vec![1.0, 0.0, -1.0]);

        let output = ConvOps::conv1d(&input, &kernel, 1, PaddingMode::Valid).unwrap();

        assert_eq!(output.shape(), &[3]);
        // [1*1 + 2*0 + 3*(-1), 2*1 + 3*0 + 4*(-1), 3*1 + 4*0 + 5*(-1)]
        // = [1 - 3, 2 - 4, 3 - 5] = [-2, -2, -2]
        assert_eq!(output.get(&[0]), Some(&-2.0));
        assert_eq!(output.get(&[1]), Some(&-2.0));
        assert_eq!(output.get(&[2]), Some(&-2.0));
    }

    #[test]
    fn test_conv1d_with_stride() {
        let input = Tensor::vector(std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let kernel = Tensor::vector(std::vec![1.0, 1.0]);

        let output = ConvOps::conv1d(&input, &kernel, 2, PaddingMode::Valid).unwrap();

        assert_eq!(output.shape(), &[3]);
        // positions 0, 2, 4 with kernel [1,1]
        // [1+2, 3+4, 5+6] = [3, 7, 11]
        assert_eq!(output.get(&[0]), Some(&3.0));
        assert_eq!(output.get(&[1]), Some(&7.0));
        assert_eq!(output.get(&[2]), Some(&11.0));
    }

    #[test]
    fn test_conv2d_valid() {
        // 3x3 input
        let input = Tensor::from_vec(
            std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
            std::vec![3, 3],
        )
        .unwrap();

        // 2x2 kernel
        let kernel = Tensor::from_vec(std::vec![1.0, 0.0, 0.0, 1.0], std::vec![2, 2]).unwrap();

        let output = ConvOps::conv2d(&input, &kernel, (1, 1), PaddingMode::Valid).unwrap();

        assert_eq!(output.shape(), &[2, 2]);
        // Top-left: 1*1 + 2*0 + 4*0 + 5*1 = 6
        // Top-right: 2*1 + 3*0 + 5*0 + 6*1 = 8
        // Bottom-left: 4*1 + 5*0 + 7*0 + 8*1 = 12
        // Bottom-right: 5*1 + 6*0 + 8*0 + 9*1 = 14
        assert_eq!(output.get(&[0, 0]), Some(&6.0));
        assert_eq!(output.get(&[0, 1]), Some(&8.0));
        assert_eq!(output.get(&[1, 0]), Some(&12.0));
        assert_eq!(output.get(&[1, 1]), Some(&14.0));
    }

    #[test]
    fn test_conv2d_same_padding() {
        // 4x4 input
        let input = Tensor::from_vec(
            std::vec![
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
                16.0,
            ],
            std::vec![4, 4],
        )
        .unwrap();

        // 3x3 kernel (identity-like)
        let kernel = Tensor::from_vec(
            std::vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
            std::vec![3, 3],
        )
        .unwrap();

        let output = ConvOps::conv2d(&input, &kernel, (1, 1), PaddingMode::Same).unwrap();

        assert_eq!(output.shape(), &[4, 4]);
        // With identity kernel centered, output should match input
        assert_eq!(output.get(&[0, 0]), Some(&1.0));
        assert_eq!(output.get(&[1, 1]), Some(&6.0));
        assert_eq!(output.get(&[3, 3]), Some(&16.0));
    }

    #[test]
    fn test_max_pool2d() {
        // 4x4 input
        let input = Tensor::from_vec(
            std::vec![
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
                16.0,
            ],
            std::vec![4, 4],
        )
        .unwrap();

        let output = ConvOps::max_pool2d(&input, (2, 2), (2, 2)).unwrap();

        assert_eq!(output.shape(), &[2, 2]);
        // Top-left 2x2: max(1,2,5,6) = 6
        // Top-right 2x2: max(3,4,7,8) = 8
        // Bottom-left 2x2: max(9,10,13,14) = 14
        // Bottom-right 2x2: max(11,12,15,16) = 16
        assert_eq!(output.get(&[0, 0]), Some(&6.0));
        assert_eq!(output.get(&[0, 1]), Some(&8.0));
        assert_eq!(output.get(&[1, 0]), Some(&14.0));
        assert_eq!(output.get(&[1, 1]), Some(&16.0));
    }

    #[test]
    fn test_avg_pool2d() {
        // 4x4 input
        let input = Tensor::from_vec(
            std::vec![
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
                16.0,
            ],
            std::vec![4, 4],
        )
        .unwrap();

        let output = ConvOps::avg_pool2d(&input, (2, 2), (2, 2)).unwrap();

        assert_eq!(output.shape(), &[2, 2]);
        // Top-left 2x2: avg(1,2,5,6) = 14/4 = 3.5
        // Top-right 2x2: avg(3,4,7,8) = 22/4 = 5.5
        // Bottom-left 2x2: avg(9,10,13,14) = 46/4 = 11.5
        // Bottom-right 2x2: avg(11,12,15,16) = 54/4 = 13.5
        assert_eq!(output.get(&[0, 0]), Some(&3.5));
        assert_eq!(output.get(&[0, 1]), Some(&5.5));
        assert_eq!(output.get(&[1, 0]), Some(&11.5));
        assert_eq!(output.get(&[1, 1]), Some(&13.5));
    }

    #[test]
    fn test_global_avg_pool2d() {
        // [1, 2, 2, 2] input - batch 1, 2 channels, 2x2 spatial
        let input = Tensor::from_vec(
            std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            std::vec![1, 2, 2, 2],
        )
        .unwrap();

        let output = ConvOps::global_avg_pool2d(&input).unwrap();

        assert_eq!(output.shape(), &[1, 2]);
        // Channel 0: avg(1,2,3,4) = 2.5
        // Channel 1: avg(5,6,7,8) = 6.5
        assert_eq!(output.get(&[0, 0]), Some(&2.5));
        assert_eq!(output.get(&[0, 1]), Some(&6.5));
    }

    #[test]
    fn test_global_max_pool2d() {
        // [1, 2, 2, 2] input - batch 1, 2 channels, 2x2 spatial
        let input = Tensor::from_vec(
            std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            std::vec![1, 2, 2, 2],
        )
        .unwrap();

        let output = ConvOps::global_max_pool2d(&input).unwrap();

        assert_eq!(output.shape(), &[1, 2]);
        // Channel 0: max(1,2,3,4) = 4
        // Channel 1: max(5,6,7,8) = 8
        assert_eq!(output.get(&[0, 0]), Some(&4.0));
        assert_eq!(output.get(&[0, 1]), Some(&8.0));
    }

    #[test]
    fn test_conv2d_batch() {
        // [1, 1, 3, 3] input - batch 1, 1 channel, 3x3 spatial
        let input = Tensor::from_vec(
            std::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
            std::vec![1, 1, 3, 3],
        )
        .unwrap();

        // [1, 1, 2, 2] kernel - 1 output channel, 1 input channel, 2x2 kernel
        let kernel =
            Tensor::from_vec(std::vec![1.0, 0.0, 0.0, 1.0], std::vec![1, 1, 2, 2]).unwrap();

        let output = ConvOps::conv2d_batch(&input, &kernel, (1, 1), PaddingMode::Valid).unwrap();

        assert_eq!(output.shape(), &[1, 1, 2, 2]);
        // Same as 2D test
        assert_eq!(output.get(&[0, 0, 0, 0]), Some(&6.0));
        assert_eq!(output.get(&[0, 0, 0, 1]), Some(&8.0));
        assert_eq!(output.get(&[0, 0, 1, 0]), Some(&12.0));
        assert_eq!(output.get(&[0, 0, 1, 1]), Some(&14.0));
    }

    #[test]
    fn test_depthwise_conv2d() {
        // [1, 2, 3, 3] input - batch 1, 2 channels, 3x3 spatial
        let input = Tensor::from_vec(
            std::vec![
                // Channel 0
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, // Channel 1
                10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0,
            ],
            std::vec![1, 2, 3, 3],
        )
        .unwrap();

        // [2, 2, 2] kernel - 2 channels, 2x2 kernel each
        let kernel = Tensor::from_vec(
            std::vec![
                // Channel 0 kernel
                1.0, 0.0, 0.0, 0.0, // Channel 1 kernel
                0.0, 0.0, 0.0, 1.0,
            ],
            std::vec![2, 2, 2],
        )
        .unwrap();

        let output =
            ConvOps::depthwise_conv2d(&input, &kernel, (1, 1), PaddingMode::Valid).unwrap();

        assert_eq!(output.shape(), &[1, 2, 2, 2]);
        // Channel 0: just top-left element of each window
        assert_eq!(output.get(&[0, 0, 0, 0]), Some(&1.0));
        assert_eq!(output.get(&[0, 0, 0, 1]), Some(&2.0));
        assert_eq!(output.get(&[0, 0, 1, 0]), Some(&4.0));
        assert_eq!(output.get(&[0, 0, 1, 1]), Some(&5.0));
        // Channel 1: just bottom-right element of each window
        assert_eq!(output.get(&[0, 1, 0, 0]), Some(&50.0));
        assert_eq!(output.get(&[0, 1, 0, 1]), Some(&60.0));
        assert_eq!(output.get(&[0, 1, 1, 0]), Some(&80.0));
        assert_eq!(output.get(&[0, 1, 1, 1]), Some(&90.0));
    }

    #[test]
    fn test_conv_error_invalid_dimensions() {
        let input_2d = Tensor::from_vec(std::vec![1.0, 2.0, 3.0, 4.0], std::vec![2, 2]).unwrap();
        let kernel_1d = Tensor::vector(std::vec![1.0, 1.0]);

        // 1D conv on 2D input should fail
        let result = ConvOps::conv1d(&input_2d, &kernel_1d, 1, PaddingMode::Valid);
        assert_eq!(result, Err(ConvError::InvalidInputDimensions));
    }

    #[test]
    fn test_conv_error_invalid_stride() {
        let input = Tensor::vector(std::vec![1.0, 2.0, 3.0]);
        let kernel = Tensor::vector(std::vec![1.0]);

        let result = ConvOps::conv1d(&input, &kernel, 0, PaddingMode::Valid);
        assert_eq!(result, Err(ConvError::InvalidStride));
    }

    #[test]
    fn test_max_pool2d_batch() {
        // [1, 1, 4, 4] input
        let input = Tensor::from_vec(
            std::vec![
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
                16.0,
            ],
            std::vec![1, 1, 4, 4],
        )
        .unwrap();

        let output = ConvOps::max_pool2d_batch(&input, (2, 2), (2, 2)).unwrap();

        assert_eq!(output.shape(), &[1, 1, 2, 2]);
        assert_eq!(output.get(&[0, 0, 0, 0]), Some(&6.0));
        assert_eq!(output.get(&[0, 0, 0, 1]), Some(&8.0));
        assert_eq!(output.get(&[0, 0, 1, 0]), Some(&14.0));
        assert_eq!(output.get(&[0, 0, 1, 1]), Some(&16.0));
    }

    #[test]
    fn test_functional_interface() {
        let input = Tensor::vector(std::vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let kernel = Tensor::vector(std::vec![1.0, 1.0]);

        let output = functional::conv1d(&input, &kernel).unwrap();
        assert_eq!(output.shape(), &[4]);
    }
}
