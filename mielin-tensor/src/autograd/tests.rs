//! Auto-generated test module (consolidated from inline `#[cfg(test)] mod` blocks)

use crate::tensor::Tensor;

use super::*;

#[cfg(test)]
mod tests_2 {
    use super::*;
    #[test]
    fn test_variable_creation() {
        let x = Variable::new(Tensor::scalar(5.0), true);
        assert_eq!(x.data().data()[0], 5.0);
        assert!(x.requires_grad());
    }
    #[test]
    fn test_add_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(3.0), true);
        let y = graph.variable(Tensor::scalar(4.0), true);
        let z = graph.add(&x, &y);
        assert_eq!(z.data().data()[0], 7.0);
        z.backward();
        assert_eq!(x.grad().unwrap().data()[0], 1.0);
        assert_eq!(y.grad().unwrap().data()[0], 1.0);
    }
    #[test]
    fn test_mul_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(3.0), true);
        let y = graph.variable(Tensor::scalar(4.0), true);
        let z = graph.mul(&x, &y);
        assert_eq!(z.data().data()[0], 12.0);
        z.backward();
        assert_eq!(x.grad().unwrap().data()[0], 4.0);
        assert_eq!(y.grad().unwrap().data()[0], 3.0);
    }
    #[test]
    fn test_sum_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::vector(alloc::vec![1.0, 2.0, 3.0]), true);
        let z = graph.sum(&x);
        assert_eq!(z.data().data()[0], 6.0);
        z.backward();
        let grad = x.grad().unwrap();
        assert_eq!(grad.data(), &[1.0, 1.0, 1.0]);
    }
    #[test]
    fn test_mean_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::vector(alloc::vec![2.0, 4.0, 6.0]), true);
        let z = graph.mean(&x);
        assert_eq!(z.data().data()[0], 4.0);
        z.backward();
        let grad = x.grad().unwrap();
        for &g in grad.data() {
            assert!((g - 1.0 / 3.0).abs() < 1e-6);
        }
    }
    #[test]
    fn test_relu_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::vector(alloc::vec![-1.0, 0.0, 2.0]), true);
        let z = graph.relu(&x);
        assert_eq!(z.data().data(), &[0.0, 0.0, 2.0]);
        z.backward_with_grad(Tensor::vector(alloc::vec![1.0, 1.0, 1.0]));
        let grad = x.grad().unwrap();
        assert_eq!(grad.data(), &[0.0, 0.0, 1.0]);
    }
    #[test]
    fn test_sigmoid_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(0.0), true);
        let z = graph.sigmoid(&x);
        assert!((z.data().data()[0] - 0.5).abs() < 1e-6);
        z.backward();
        assert!((x.grad().unwrap().data()[0] - 0.25).abs() < 1e-6);
    }
    #[test]
    fn test_tanh_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(0.0), true);
        let z = graph.tanh(&x);
        assert!((z.data().data()[0]).abs() < 1e-6);
        z.backward();
        assert!((x.grad().unwrap().data()[0] - 1.0).abs() < 1e-6);
    }
    #[test]
    fn test_pow_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let z = graph.pow(&x, 3.0);
        assert_eq!(z.data().data()[0], 8.0);
        z.backward();
        assert_eq!(x.grad().unwrap().data()[0], 12.0);
    }
    #[test]
    fn test_exp_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(1.0), true);
        let z = graph.exp(&x);
        assert!((z.data().data()[0] - libm::expf(1.0)).abs() < 1e-6);
        z.backward();
        assert!((x.grad().unwrap().data()[0] - libm::expf(1.0)).abs() < 1e-6);
    }
    #[test]
    fn test_log_gradient() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let z = graph.log(&x);
        assert!((z.data().data()[0] - libm::logf(2.0)).abs() < 1e-6);
        z.backward();
        assert!((x.grad().unwrap().data()[0] - 0.5).abs() < 1e-6);
    }
    #[test]
    fn test_chain_rule() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let y = graph.variable(Tensor::scalar(3.0), true);
        let sum = graph.add(&x, &y);
        let z = graph.mul(&sum, &x);
        assert_eq!(z.data().data()[0], 10.0);
        z.backward();
        assert_eq!(x.grad().unwrap().data()[0], 7.0);
        assert_eq!(y.grad().unwrap().data()[0], 2.0);
    }
    #[test]
    fn test_complex_computation() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(3.0), true);
        let x_squared = graph.pow(&x, 2.0);
        let two_x = graph.mul(&x, &graph.variable(Tensor::scalar(2.0), false));
        let sum1 = graph.add(&x_squared, &two_x);
        let y = graph.add(&sum1, &graph.variable(Tensor::scalar(1.0), false));
        assert_eq!(y.data().data()[0], 16.0);
        y.backward();
        assert_eq!(x.grad().unwrap().data()[0], 8.0);
    }
    #[test]
    fn test_zero_grad() {
        let graph = ComputeGraph::new();
        let mut x = graph.variable(Tensor::scalar(5.0), true);
        let z = graph.pow(&x, 2.0);
        z.backward();
        assert!(x.grad().is_some());
        x.zero_grad();
        assert!(x.grad().is_none());
    }
    #[test]
    fn test_matmul_matrix_vector() {
        let graph = ComputeGraph::new();
        let a = graph.variable(
            Tensor::from_vec(alloc::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], alloc::vec![2, 3]).unwrap(),
            true,
        );
        let b = graph.variable(Tensor::vector(alloc::vec![1.0, 2.0, 3.0]), true);
        let c = graph.matmul(&a, &b);
        assert_eq!(c.data().data(), &[14.0, 32.0]);
        c.backward_with_grad(Tensor::vector(alloc::vec![1.0, 1.0]));
        let a_grad = a.grad().unwrap();
        assert_eq!(a_grad.data(), &[1.0, 2.0, 3.0, 1.0, 2.0, 3.0]);
        let b_grad = b.grad().unwrap();
        assert_eq!(b_grad.data(), &[5.0, 7.0, 9.0]);
    }
    #[test]
    fn test_matmul_matrix_matrix() {
        let graph = ComputeGraph::new();
        let a = graph.variable(
            Tensor::from_vec(alloc::vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], alloc::vec![2, 3]).unwrap(),
            true,
        );
        let b = graph.variable(
            Tensor::from_vec(
                alloc::vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0],
                alloc::vec![3, 2],
            )
            .unwrap(),
            true,
        );
        let c = graph.matmul(&a, &b);
        assert_eq!(c.data().shape(), &[2, 2]);
        assert_eq!(c.data().get(&[0, 0]), Some(&58.0));
        assert_eq!(c.data().get(&[0, 1]), Some(&64.0));
        assert_eq!(c.data().get(&[1, 0]), Some(&139.0));
        assert_eq!(c.data().get(&[1, 1]), Some(&154.0));
        c.backward_with_grad(
            Tensor::from_vec(alloc::vec![1.0, 1.0, 1.0, 1.0], alloc::vec![2, 2]).unwrap(),
        );
        assert!(a.grad().is_some());
        assert!(b.grad().is_some());
    }
    #[test]
    fn test_matmul_gradient_simple() {
        let graph = ComputeGraph::new();
        let a = graph.variable(
            Tensor::from_vec(alloc::vec![2.0, 3.0], alloc::vec![1, 2]).unwrap(),
            true,
        );
        let b = graph.variable(Tensor::vector(alloc::vec![4.0, 5.0]), true);
        let c = graph.matmul(&a, &b);
        assert_eq!(c.data().data()[0], 23.0);
        c.backward();
        let a_grad = a.grad().unwrap();
        assert_eq!(a_grad.data(), &[4.0, 5.0]);
        let b_grad = b.grad().unwrap();
        assert_eq!(b_grad.data(), &[2.0, 3.0]);
    }
    #[test]
    fn test_checkpoint_flag() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(5.0), true);
        let mut y = graph.add(&x, &graph.variable(Tensor::scalar(3.0), false));
        assert!(!y.is_checkpoint());
        y.checkpoint();
        assert!(y.is_checkpoint());
    }
    #[test]
    fn test_checkpoint_enable_disable() {
        let graph = ComputeGraph::new();
        assert!(!graph.is_checkpointing_enabled());
        graph.enable_checkpointing();
        assert!(graph.is_checkpointing_enabled());
        graph.disable_checkpointing();
        assert!(!graph.is_checkpointing_enabled());
    }
    #[test]
    fn test_checkpoint_gradient_computation() {
        let graph = ComputeGraph::new();
        graph.enable_checkpointing();
        let x = graph.variable(Tensor::scalar(3.0), true);
        let y = graph.variable(Tensor::scalar(4.0), true);
        let mut z = graph.mul(&x, &y);
        z.checkpoint();
        assert_eq!(z.data().data()[0], 12.0);
        z.backward();
        assert_eq!(x.grad().unwrap().data()[0], 4.0);
        assert_eq!(y.grad().unwrap().data()[0], 3.0);
    }
    #[test]
    fn test_checkpoint_clear_cache() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(5.0), true);
        let mut y = graph.pow(&x, 2.0);
        assert_eq!(y.data().data()[0], 25.0);
        y.checkpoint();
        y.clear_cache();
        assert_eq!(y.data().data()[0], 0.0);
    }
    #[test]
    fn test_checkpoint_multiple_nodes() {
        let graph = ComputeGraph::new();
        graph.enable_checkpointing();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let mut y1 = graph.mul(&x, &x);
        let mut y2 = graph.add(&y1, &x);
        let z = graph.mul(&y2, &x);
        y1.checkpoint();
        y2.checkpoint();
        assert!(y1.is_checkpoint());
        assert!(y2.is_checkpoint());
        assert_eq!(z.data().data()[0], 12.0);
        z.backward();
        assert_eq!(x.grad().unwrap().data()[0], 16.0);
    }
    #[test]
    fn test_grad_var() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(3.0), true);
        let y = graph.pow(&x, 2.0);
        y.backward();
        let grad_var = x.grad_var(false);
        assert!(grad_var.is_some());
        assert_eq!(grad_var.unwrap().data().data()[0], 6.0);
    }
    #[test]
    fn test_higher_order_gradient_manual() {
        let graph1 = ComputeGraph::new();
        let x1 = graph1.variable(Tensor::scalar(2.0), true);
        let y1 = graph1.pow(&x1, 3.0);
        y1.backward();
        let first_deriv = x1.grad().unwrap().data()[0];
        assert_eq!(first_deriv, 12.0);
        let graph2 = ComputeGraph::new();
        let x2 = graph2.variable(Tensor::scalar(2.0), true);
        let grad_func = graph2.pow(&x2, 2.0);
        let scaled = graph2.mul(&grad_func, &graph2.variable(Tensor::scalar(3.0), false));
        scaled.backward();
        let second_deriv = x2.grad().unwrap().data()[0];
        assert_eq!(second_deriv, 12.0);
    }
    #[test]
    fn test_grad_and_detach() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(5.0), true);
        let y = graph.mul(&x, &x);
        y.backward();
        let grad_detached = x.grad_and_detach();
        assert!(grad_detached.is_some());
        assert!(!grad_detached.unwrap().requires_grad());
        assert_eq!(x.grad().unwrap().data()[0], 10.0);
    }
    #[test]
    fn test_jvp_correctness() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let y = graph.pow(&x, 2.0);
        let v = Tensor::scalar(1.0);
        let jvp_result = graph.jvp(&y, &x, &v);
        assert_eq!(jvp_result.data().len(), 1);
        let expected = 4.0_f32;
        assert!(
            (jvp_result.data()[0] - expected).abs() < 1e-5,
            "JVP of x^2 at x=2, v=1 should be 4.0, got {}",
            jvp_result.data()[0]
        );
    }
    #[test]
    fn test_jvp_linear() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(3.0), true);
        let c = graph.variable(Tensor::scalar(1.0), false);
        let y = graph.mul(&x, &c);
        let v = Tensor::scalar(1.0);
        let jvp_result = graph.jvp(&y, &x, &v);
        assert!(
            (jvp_result.data()[0] - 1.0).abs() < 1e-5,
            "JVP of linear y=x*1 at v=1 should be 1.0, got {}",
            jvp_result.data()[0]
        );
    }
    #[test]
    fn test_jvp_square() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let y = graph.pow(&x, 2.0);
        let v = Tensor::scalar(1.0);
        let result = graph.jvp(&y, &x, &v);
        assert!((result.data()[0] - 4.0).abs() < 1e-5);
    }
    #[test]
    fn test_jvp_exp() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(0.0), true);
        let y = graph.exp(&x);
        let v = Tensor::scalar(1.0);
        let result = graph.jvp(&y, &x, &v);
        assert!(
            (result.data()[0] - 1.0).abs() < 1e-5,
            "JVP of exp(x) at x=0, v=1 should be 1.0, got {}",
            result.data()[0]
        );
    }
    #[test]
    fn test_jvp_log() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let y = graph.log(&x);
        let v = Tensor::scalar(1.0);
        let result = graph.jvp(&y, &x, &v);
        assert!(
            (result.data()[0] - 0.5).abs() < 1e-5,
            "JVP of log(x) at x=2, v=1 should be 0.5, got {}",
            result.data()[0]
        );
    }
    #[test]
    fn test_jvp_chain() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(1.0), true);
        let x_sq = graph.pow(&x, 2.0);
        let y = graph.exp(&x_sq);
        let v = Tensor::scalar(1.0);
        let result = graph.jvp(&y, &x, &v);
        let expected = 2.0 * libm::expf(1.0);
        assert!(
            (result.data()[0] - expected).abs() < 1e-4,
            "JVP of exp(x^2) at x=1, v=1 should be ~{}, got {}",
            expected,
            result.data()[0]
        );
    }
    #[test]
    fn test_jvp_matmul() {
        let graph = ComputeGraph::new();
        let a = graph.variable(
            Tensor::from_vec(alloc::vec![1.0, 2.0, 3.0, 4.0], alloc::vec![2, 2]).unwrap(),
            false,
        );
        let b = graph.variable(Tensor::vector(alloc::vec![1.0, 0.0]), true);
        let y = graph.matmul(&a, &b);
        let v = Tensor::vector(alloc::vec![1.0, 1.0]);
        let result = graph.jvp(&y, &b, &v);
        assert_eq!(result.data().len(), 2);
        assert!(
            (result.data()[0] - 3.0).abs() < 1e-5,
            "got {}",
            result.data()[0]
        );
        assert!(
            (result.data()[1] - 7.0).abs() < 1e-5,
            "got {}",
            result.data()[1]
        );
    }
    #[test]
    fn test_jvp_finite_diff_crosscheck() {
        let eps = 1e-4_f32;
        let x_val = 1.5_f32;
        let v_val = 1.0_f32;
        let f = |xv: f32| {
            let xsq = xv * xv;
            1.0 / (1.0 + libm::expf(-xsq))
        };
        let fd_jvp = (f(x_val + eps * v_val) - f(x_val)) / eps;
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(x_val), true);
        let x_sq = graph.pow(&x, 2.0);
        let y = graph.sigmoid(&x_sq);
        let v = Tensor::scalar(v_val);
        let ad_jvp = graph.jvp(&y, &x, &v).data()[0];
        assert!(
            (ad_jvp - fd_jvp).abs() < 1e-3,
            "JVP {} vs finite diff {} differ by more than 1e-3",
            ad_jvp,
            fd_jvp
        );
    }
    #[test]
    fn test_second_derivative_square() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let y = graph.pow(&x, 2.0);
        let second = graph.second_derivative(&y, &x);
        assert!(second.is_some());
        let val = second.unwrap();
        assert!(
            (val - 2.0).abs() < 1e-5,
            "d²(x²)/dx² should be 2.0, got {}",
            val
        );
    }
    #[test]
    fn test_second_derivative_cube() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(2.0), true);
        let y = graph.pow(&x, 3.0);
        let second = graph.second_derivative(&y, &x);
        assert!(second.is_some());
        let val = second.unwrap();
        assert!(
            (val - 12.0).abs() < 1e-4,
            "d²(x³)/dx² at x=2 should be 12.0, got {}",
            val
        );
    }
    #[test]
    fn test_second_derivative_exp() {
        let graph = ComputeGraph::new();
        let x = graph.variable(Tensor::scalar(0.0), true);
        let y = graph.exp(&x);
        let second = graph.second_derivative(&y, &x);
        assert!(second.is_some());
        let val = second.unwrap();
        assert!(
            (val - 1.0).abs() < 1e-5,
            "d²(exp(x))/dx² at x=0 should be 1.0, got {}",
            val
        );
    }
}
