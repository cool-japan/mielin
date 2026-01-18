//! Complex number support for tensors
//!
//! Provides complex number types and operations for tensor computations.
//! Supports both f32 (Complex32) and f64 (Complex64) precision.
//!
//! # Examples
//!
//! ```
//! use mielin_tensor::complex::{Complex32, ComplexTensor};
//! use mielin_tensor::Tensor;
//!
//! // Create complex numbers
//! let z1 = Complex32::new(3.0, 4.0);  // 3 + 4i
//! let z2 = Complex32::new(1.0, 2.0);  // 1 + 2i
//!
//! // Complex arithmetic
//! let sum = z1 + z2;
//! let product = z1 * z2;
//!
//! // Complex tensor
//! let data = vec![
//!     Complex32::new(1.0, 0.0),
//!     Complex32::new(0.0, 1.0),
//!     Complex32::new(-1.0, 0.0),
//! ];
//! let tensor = ComplexTensor::from_vec(data, vec![3]);
//! ```

#![allow(dead_code)]

use crate::error::{TensorError, TensorResult};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::ops::{Add, Div, Mul, Neg, Sub};

/// Complex number with f32 components
pub type Complex32 = Complex<f32>;

/// Complex number with f64 components
pub type Complex64 = Complex<f64>;

/// Generic complex number representation
///
/// Stores a complex number as real and imaginary parts.
/// Supports arithmetic operations and complex-specific functions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Complex<T> {
    /// Real part
    pub re: T,
    /// Imaginary part
    pub im: T,
}

impl<T> Complex<T> {
    /// Create a new complex number from real and imaginary parts
    ///
    /// # Examples
    ///
    /// ```
    /// # use mielin_tensor::complex::Complex32;
    /// let z = Complex32::new(3.0, 4.0);  // 3 + 4i
    /// assert_eq!(z.re, 3.0);
    /// assert_eq!(z.im, 4.0);
    /// ```
    #[inline]
    pub const fn new(re: T, im: T) -> Self {
        Self { re, im }
    }
}

impl<T: Copy> Complex<T> {
    /// Get the real part
    #[inline]
    pub fn real(&self) -> T {
        self.re
    }

    /// Get the imaginary part
    #[inline]
    pub fn imag(&self) -> T {
        self.im
    }
}

// Float-specific operations
impl Complex<f32> {
    /// Create a complex number from polar coordinates (r, theta)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mielin_tensor::complex::Complex32;
    /// let z = Complex32::from_polar(1.0, core::f32::consts::PI / 2.0);
    /// assert!((z.re).abs() < 1e-6);
    /// assert!((z.im - 1.0).abs() < 1e-6);
    /// ```
    #[inline]
    pub fn from_polar(r: f32, theta: f32) -> Self {
        Self::new(r * libm::cosf(theta), r * libm::sinf(theta))
    }

    /// Compute the magnitude (absolute value) of the complex number
    ///
    /// # Examples
    ///
    /// ```
    /// # use mielin_tensor::complex::Complex32;
    /// let z = Complex32::new(3.0, 4.0);
    /// assert_eq!(z.abs(), 5.0);
    /// ```
    #[inline]
    pub fn abs(&self) -> f32 {
        libm::hypotf(self.re, self.im)
    }

    /// Compute the squared magnitude (|z|^2)
    ///
    /// This is faster than abs() as it avoids the square root.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mielin_tensor::complex::Complex32;
    /// let z = Complex32::new(3.0, 4.0);
    /// assert_eq!(z.abs_sqr(), 25.0);
    /// ```
    #[inline]
    pub fn abs_sqr(&self) -> f32 {
        self.re * self.re + self.im * self.im
    }

    /// Compute the argument (phase angle) of the complex number in radians
    ///
    /// # Examples
    ///
    /// ```
    /// # use mielin_tensor::complex::Complex32;
    /// let z = Complex32::new(1.0, 1.0);
    /// let arg = z.arg();
    /// assert!((arg - core::f32::consts::PI / 4.0).abs() < 1e-6);
    /// ```
    #[inline]
    pub fn arg(&self) -> f32 {
        libm::atan2f(self.im, self.re)
    }

    /// Return the complex conjugate (re, -im)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mielin_tensor::complex::Complex32;
    /// let z = Complex32::new(3.0, 4.0);
    /// let conj = z.conj();
    /// assert_eq!(conj.re, 3.0);
    /// assert_eq!(conj.im, -4.0);
    /// ```
    #[inline]
    pub fn conj(&self) -> Self {
        Self::new(self.re, -self.im)
    }

    /// Compute the reciprocal (1/z)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mielin_tensor::complex::Complex32;
    /// let z = Complex32::new(3.0, 4.0);
    /// let inv = z.inv();
    /// assert!((inv.re - 0.12).abs() < 1e-6);
    /// assert!((inv.im + 0.16).abs() < 1e-6);
    /// ```
    #[inline]
    pub fn inv(&self) -> Self {
        let norm_sqr = self.abs_sqr();
        Self::new(self.re / norm_sqr, -self.im / norm_sqr)
    }

    /// Compute e^z (complex exponential)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mielin_tensor::complex::Complex32;
    /// let z = Complex32::new(0.0, core::f32::consts::PI);
    /// let exp_z = z.exp();
    /// assert!((exp_z.re + 1.0).abs() < 1e-6);
    /// assert!(exp_z.im.abs() < 1e-6);
    /// ```
    #[inline]
    pub fn exp(&self) -> Self {
        let exp_re = libm::expf(self.re);
        Self::new(exp_re * libm::cosf(self.im), exp_re * libm::sinf(self.im))
    }

    /// Compute ln(z) (complex natural logarithm)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mielin_tensor::complex::Complex32;
    /// let z = Complex32::new(1.0, 0.0);
    /// let ln_z = z.ln();
    /// assert!(ln_z.re.abs() < 1e-6);
    /// assert!(ln_z.im.abs() < 1e-6);
    /// ```
    #[inline]
    pub fn ln(&self) -> Self {
        Self::new(libm::logf(self.abs()), self.arg())
    }

    /// Compute sqrt(z) (complex square root)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mielin_tensor::complex::Complex32;
    /// let z = Complex32::new(0.0, 4.0);
    /// let sqrt_z = z.sqrt();
    /// assert!((sqrt_z.re - libm::sqrtf(2.0)).abs() < 1e-6);
    /// assert!((sqrt_z.im - libm::sqrtf(2.0)).abs() < 1e-6);
    /// ```
    #[inline]
    pub fn sqrt(&self) -> Self {
        let r = self.abs();
        let theta = self.arg();
        Self::from_polar(libm::sqrtf(r), theta / 2.0)
    }

    /// Compute z^n (complex power with integer exponent)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mielin_tensor::complex::Complex32;
    /// let z = Complex32::new(1.0, 1.0);
    /// let z_squared = z.powi(2);
    /// assert!((z_squared.re).abs() < 1e-6);
    /// assert!((z_squared.im - 2.0).abs() < 1e-6);
    /// ```
    pub fn powi(&self, n: i32) -> Self {
        if n == 0 {
            return Self::new(1.0, 0.0);
        }
        if n < 0 {
            return self.inv().powi(-n);
        }

        let mut result = Self::new(1.0, 0.0);
        let mut base = *self;
        let mut exp = n;

        while exp > 0 {
            if exp % 2 == 1 {
                result = result * base;
            }
            base = base * base;
            exp /= 2;
        }

        result
    }
}

impl Complex<f64> {
    /// Create a complex number from polar coordinates (r, theta)
    #[inline]
    pub fn from_polar(r: f64, theta: f64) -> Self {
        Self::new(r * libm::cos(theta), r * libm::sin(theta))
    }

    /// Compute the magnitude (absolute value)
    #[inline]
    pub fn abs(&self) -> f64 {
        libm::hypot(self.re, self.im)
    }

    /// Compute the squared magnitude
    #[inline]
    pub fn abs_sqr(&self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    /// Compute the argument (phase angle)
    #[inline]
    pub fn arg(&self) -> f64 {
        libm::atan2(self.im, self.re)
    }

    /// Return the complex conjugate
    #[inline]
    pub fn conj(&self) -> Self {
        Self::new(self.re, -self.im)
    }

    /// Compute the reciprocal
    #[inline]
    pub fn inv(&self) -> Self {
        let norm_sqr = self.abs_sqr();
        Self::new(self.re / norm_sqr, -self.im / norm_sqr)
    }

    /// Compute e^z
    #[inline]
    pub fn exp(&self) -> Self {
        let exp_re = libm::exp(self.re);
        Self::new(exp_re * libm::cos(self.im), exp_re * libm::sin(self.im))
    }

    /// Compute ln(z)
    #[inline]
    pub fn ln(&self) -> Self {
        Self::new(libm::log(self.abs()), self.arg())
    }

    /// Compute sqrt(z)
    #[inline]
    pub fn sqrt(&self) -> Self {
        let r = self.abs();
        let theta = self.arg();
        Self::from_polar(libm::sqrt(r), theta / 2.0)
    }

    /// Compute z^n
    pub fn powi(&self, n: i32) -> Self {
        if n == 0 {
            return Self::new(1.0, 0.0);
        }
        if n < 0 {
            return self.inv().powi(-n);
        }

        let mut result = Self::new(1.0, 0.0);
        let mut base = *self;
        let mut exp = n;

        while exp > 0 {
            if exp % 2 == 1 {
                result = result * base;
            }
            base = base * base;
            exp /= 2;
        }

        result
    }
}

// Arithmetic operations
impl<T: Add<Output = T>> Add for Complex<T> {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.re + rhs.re, self.im + rhs.im)
    }
}

impl<T: Sub<Output = T>> Sub for Complex<T> {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.re - rhs.re, self.im - rhs.im)
    }
}

impl<T: Copy + Add<Output = T> + Sub<Output = T> + Mul<Output = T>> Mul for Complex<T> {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        // (a + bi)(c + di) = (ac - bd) + (ad + bc)i
        Self::new(
            self.re * rhs.re - self.im * rhs.im,
            self.re * rhs.im + self.im * rhs.re,
        )
    }
}

impl Div for Complex<f32> {
    type Output = Self;

    #[inline]
    fn div(self, rhs: Self) -> Self::Output {
        let norm_sqr = rhs.abs_sqr();
        Self::new(
            (self.re * rhs.re + self.im * rhs.im) / norm_sqr,
            (self.im * rhs.re - self.re * rhs.im) / norm_sqr,
        )
    }
}

impl Div for Complex<f64> {
    type Output = Self;

    #[inline]
    fn div(self, rhs: Self) -> Self::Output {
        let norm_sqr = rhs.abs_sqr();
        Self::new(
            (self.re * rhs.re + self.im * rhs.im) / norm_sqr,
            (self.im * rhs.re - self.re * rhs.im) / norm_sqr,
        )
    }
}

impl<T: Neg<Output = T>> Neg for Complex<T> {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self::Output {
        Self::new(-self.re, -self.im)
    }
}

// Display implementation
impl<T: fmt::Display> fmt::Display for Complex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} + {}i", self.re, self.im)
    }
}

/// Complex tensor for multi-dimensional complex arrays
///
/// Stores complex numbers in interleaved format (re, im, re, im, ...)
/// for cache efficiency.
#[derive(Debug, Clone)]
pub struct ComplexTensor<T> {
    data: Vec<Complex<T>>,
    shape: Vec<usize>,
}

impl<T: Copy> ComplexTensor<T> {
    /// Create a complex tensor from a vector and shape
    ///
    /// # Errors
    ///
    /// Returns an error if the shape doesn't match the data length.
    pub fn from_vec(data: Vec<Complex<T>>, shape: Vec<usize>) -> TensorResult<Self> {
        let total_size: usize = shape.iter().product();
        if total_size != data.len() {
            return Err(TensorError::invalid_reshape(vec![data.len()], shape));
        }
        Ok(Self { data, shape })
    }

    /// Create a complex tensor filled with a single value
    pub fn filled(shape: Vec<usize>, value: Complex<T>) -> Self {
        let total_size: usize = shape.iter().product();
        Self {
            data: vec![value; total_size],
            shape,
        }
    }

    /// Get the shape of the tensor
    #[inline]
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get the total number of elements
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the tensor is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get a reference to the underlying data
    #[inline]
    pub fn data(&self) -> &[Complex<T>] {
        &self.data
    }

    /// Get a mutable reference to the underlying data
    #[inline]
    pub fn data_mut(&mut self) -> &mut [Complex<T>] {
        &mut self.data
    }

    /// Get an element at a specific index
    pub fn get(&self, index: usize) -> Option<Complex<T>> {
        self.data.get(index).copied()
    }

    /// Set an element at a specific index
    pub fn set(&mut self, index: usize, value: Complex<T>) -> TensorResult<()> {
        if index >= self.data.len() {
            return Err(TensorError::IndexOutOfBounds {
                indices: vec![index],
                shape: vec![self.data.len()],
            });
        }
        self.data[index] = value;
        Ok(())
    }
}

impl ComplexTensor<f32> {
    /// Element-wise addition of two complex tensors
    pub fn add(&self, other: &Self) -> TensorResult<Self> {
        if self.shape != other.shape {
            return Err(TensorError::shape_mismatch(
                "complex_add",
                self.shape.clone(),
                other.shape.clone(),
            ));
        }

        let data: Vec<Complex<f32>> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| *a + *b)
            .collect();

        Ok(Self {
            data,
            shape: self.shape.clone(),
        })
    }

    /// Element-wise multiplication of two complex tensors
    pub fn mul(&self, other: &Self) -> TensorResult<Self> {
        if self.shape != other.shape {
            return Err(TensorError::shape_mismatch(
                "complex_mul",
                self.shape.clone(),
                other.shape.clone(),
            ));
        }

        let data: Vec<Complex<f32>> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| *a * *b)
            .collect();

        Ok(Self {
            data,
            shape: self.shape.clone(),
        })
    }

    /// Compute the complex conjugate of all elements
    pub fn conj(&self) -> Self {
        let data: Vec<Complex<f32>> = self.data.iter().map(|z| z.conj()).collect();
        Self {
            data,
            shape: self.shape.clone(),
        }
    }

    /// Compute the magnitude of all elements
    pub fn abs(&self) -> Vec<f32> {
        self.data.iter().map(|z| z.abs()).collect()
    }

    /// Compute the phase of all elements
    pub fn arg(&self) -> Vec<f32> {
        self.data.iter().map(|z| z.arg()).collect()
    }

    /// Scale all elements by a complex scalar
    pub fn scale(&self, scalar: Complex<f32>) -> Self {
        let data: Vec<Complex<f32>> = self.data.iter().map(|z| *z * scalar).collect();
        Self {
            data,
            shape: self.shape.clone(),
        }
    }
}

impl ComplexTensor<f64> {
    /// Element-wise addition of two complex tensors
    pub fn add(&self, other: &Self) -> TensorResult<Self> {
        if self.shape != other.shape {
            return Err(TensorError::shape_mismatch(
                "complex_add",
                self.shape.clone(),
                other.shape.clone(),
            ));
        }

        let data: Vec<Complex<f64>> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| *a + *b)
            .collect();

        Ok(Self {
            data,
            shape: self.shape.clone(),
        })
    }

    /// Element-wise multiplication of two complex tensors
    pub fn mul(&self, other: &Self) -> TensorResult<Self> {
        if self.shape != other.shape {
            return Err(TensorError::shape_mismatch(
                "complex_mul",
                self.shape.clone(),
                other.shape.clone(),
            ));
        }

        let data: Vec<Complex<f64>> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| *a * *b)
            .collect();

        Ok(Self {
            data,
            shape: self.shape.clone(),
        })
    }

    /// Compute the complex conjugate of all elements
    pub fn conj(&self) -> Self {
        let data: Vec<Complex<f64>> = self.data.iter().map(|z| z.conj()).collect();
        Self {
            data,
            shape: self.shape.clone(),
        }
    }

    /// Compute the magnitude of all elements
    pub fn abs(&self) -> Vec<f64> {
        self.data.iter().map(|z| z.abs()).collect()
    }

    /// Compute the phase of all elements
    pub fn arg(&self) -> Vec<f64> {
        self.data.iter().map(|z| z.arg()).collect()
    }

    /// Scale all elements by a complex scalar
    pub fn scale(&self, scalar: Complex<f64>) -> Self {
        let data: Vec<Complex<f64>> = self.data.iter().map(|z| *z * scalar).collect();
        Self {
            data,
            shape: self.shape.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    const EPSILON: f32 = 1e-6;

    #[test]
    fn test_complex_creation() {
        let z = Complex32::new(3.0, 4.0);
        assert_eq!(z.re, 3.0);
        assert_eq!(z.im, 4.0);
    }

    #[test]
    fn test_complex_addition() {
        let z1 = Complex32::new(1.0, 2.0);
        let z2 = Complex32::new(3.0, 4.0);
        let sum = z1 + z2;
        assert_eq!(sum.re, 4.0);
        assert_eq!(sum.im, 6.0);
    }

    #[test]
    fn test_complex_subtraction() {
        let z1 = Complex32::new(5.0, 7.0);
        let z2 = Complex32::new(2.0, 3.0);
        let diff = z1 - z2;
        assert_eq!(diff.re, 3.0);
        assert_eq!(diff.im, 4.0);
    }

    #[test]
    fn test_complex_multiplication() {
        let z1 = Complex32::new(3.0, 2.0);
        let z2 = Complex32::new(1.0, 4.0);
        let product = z1 * z2;
        // (3 + 2i)(1 + 4i) = 3 + 12i + 2i + 8i^2 = 3 + 14i - 8 = -5 + 14i
        assert_eq!(product.re, -5.0);
        assert_eq!(product.im, 14.0);
    }

    #[test]
    fn test_complex_division() {
        let z1 = Complex32::new(3.0, 4.0);
        let z2 = Complex32::new(1.0, 0.0);
        let quotient = z1 / z2;
        assert_eq!(quotient.re, 3.0);
        assert_eq!(quotient.im, 4.0);
    }

    #[test]
    fn test_complex_abs() {
        let z = Complex32::new(3.0, 4.0);
        assert_eq!(z.abs(), 5.0);
    }

    #[test]
    fn test_complex_abs_sqr() {
        let z = Complex32::new(3.0, 4.0);
        assert_eq!(z.abs_sqr(), 25.0);
    }

    #[test]
    fn test_complex_conj() {
        let z = Complex32::new(3.0, 4.0);
        let conj = z.conj();
        assert_eq!(conj.re, 3.0);
        assert_eq!(conj.im, -4.0);
    }

    #[test]
    fn test_complex_inv() {
        let z = Complex32::new(3.0, 4.0);
        let inv = z.inv();
        assert!((inv.re - 0.12).abs() < EPSILON);
        assert!((inv.im + 0.16).abs() < EPSILON);
    }

    #[test]
    fn test_complex_exp() {
        let z = Complex32::new(0.0, core::f32::consts::PI);
        let exp_z = z.exp();
        assert!((exp_z.re + 1.0).abs() < EPSILON);
        assert!(exp_z.im.abs() < EPSILON);
    }

    #[test]
    fn test_complex_ln() {
        let z = Complex32::new(1.0, 0.0);
        let ln_z = z.ln();
        assert!(ln_z.re.abs() < EPSILON);
        assert!(ln_z.im.abs() < EPSILON);
    }

    #[test]
    fn test_complex_sqrt() {
        let z = Complex32::new(0.0, 4.0);
        let sqrt_z = z.sqrt();
        let expected = libm::sqrtf(2.0);
        assert!((sqrt_z.re - expected).abs() < EPSILON);
        assert!((sqrt_z.im - expected).abs() < EPSILON);
    }

    #[test]
    fn test_complex_powi() {
        let z = Complex32::new(1.0, 1.0);
        let z_squared = z.powi(2);
        assert!(z_squared.re.abs() < EPSILON);
        assert!((z_squared.im - 2.0).abs() < EPSILON);
    }

    #[test]
    fn test_complex_from_polar() {
        let z = Complex32::from_polar(1.0, core::f32::consts::PI / 2.0);
        assert!(z.re.abs() < EPSILON);
        assert!((z.im - 1.0).abs() < EPSILON);
    }

    #[test]
    fn test_complex_arg() {
        let z = Complex32::new(1.0, 1.0);
        let arg = z.arg();
        assert!((arg - core::f32::consts::PI / 4.0).abs() < EPSILON);
    }

    #[test]
    fn test_complex_tensor_creation() {
        let data = vec![
            Complex32::new(1.0, 0.0),
            Complex32::new(0.0, 1.0),
            Complex32::new(-1.0, 0.0),
        ];
        let tensor = ComplexTensor::from_vec(data, vec![3]).unwrap();
        assert_eq!(tensor.len(), 3);
        assert_eq!(tensor.shape(), &[3]);
    }

    #[test]
    fn test_complex_tensor_add() {
        let t1 = ComplexTensor::from_vec(
            vec![Complex32::new(1.0, 2.0), Complex32::new(3.0, 4.0)],
            vec![2],
        )
        .unwrap();
        let t2 = ComplexTensor::from_vec(
            vec![Complex32::new(5.0, 6.0), Complex32::new(7.0, 8.0)],
            vec![2],
        )
        .unwrap();

        let result = t1.add(&t2).unwrap();
        assert_eq!(result.data()[0], Complex32::new(6.0, 8.0));
        assert_eq!(result.data()[1], Complex32::new(10.0, 12.0));
    }

    #[test]
    fn test_complex_tensor_mul() {
        let t1 = ComplexTensor::from_vec(
            vec![Complex32::new(1.0, 2.0), Complex32::new(3.0, 4.0)],
            vec![2],
        )
        .unwrap();
        let t2 = ComplexTensor::from_vec(
            vec![Complex32::new(1.0, 0.0), Complex32::new(0.0, 1.0)],
            vec![2],
        )
        .unwrap();

        let result = t1.mul(&t2).unwrap();
        assert_eq!(result.data()[0], Complex32::new(1.0, 2.0));
        assert_eq!(result.data()[1], Complex32::new(-4.0, 3.0));
    }

    #[test]
    fn test_complex_tensor_conj() {
        let tensor = ComplexTensor::from_vec(
            vec![Complex32::new(1.0, 2.0), Complex32::new(3.0, -4.0)],
            vec![2],
        )
        .unwrap();

        let conj = tensor.conj();
        assert_eq!(conj.data()[0], Complex32::new(1.0, -2.0));
        assert_eq!(conj.data()[1], Complex32::new(3.0, 4.0));
    }

    #[test]
    fn test_complex_tensor_abs() {
        let tensor = ComplexTensor::from_vec(
            vec![Complex32::new(3.0, 4.0), Complex32::new(5.0, 12.0)],
            vec![2],
        )
        .unwrap();

        let abs_values = tensor.abs();
        assert_eq!(abs_values[0], 5.0);
        assert_eq!(abs_values[1], 13.0);
    }

    #[test]
    fn test_complex_tensor_scale() {
        let tensor = ComplexTensor::from_vec(
            vec![Complex32::new(1.0, 2.0), Complex32::new(3.0, 4.0)],
            vec![2],
        )
        .unwrap();

        let scaled = tensor.scale(Complex32::new(2.0, 0.0));
        assert_eq!(scaled.data()[0], Complex32::new(2.0, 4.0));
        assert_eq!(scaled.data()[1], Complex32::new(6.0, 8.0));
    }

    #[test]
    fn test_complex_negation() {
        let z = Complex32::new(3.0, -4.0);
        let neg_z = -z;
        assert_eq!(neg_z.re, -3.0);
        assert_eq!(neg_z.im, 4.0);
    }

    #[test]
    fn test_complex64_operations() {
        let z1 = Complex64::new(1.0, 2.0);
        let z2 = Complex64::new(3.0, 4.0);

        let sum = z1 + z2;
        assert_eq!(sum.re, 4.0);
        assert_eq!(sum.im, 6.0);

        let product = z1 * z2;
        assert_eq!(product.re, -5.0);
        assert_eq!(product.im, 10.0);
    }
}
