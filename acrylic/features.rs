#[cfg(not(any(feature = "ssaa-x2", feature = "ssaa-x3", feature = "ssaa-x4")))]
/// Super-Sampling Anti-Aliasing
pub const SSAA: usize = 1;

#[cfg(feature = "ssaa-x2")]
/// Super-Sampling Anti-Aliasing
pub const SSAA: usize = 2;

#[cfg(feature = "ssaa-x3")]
/// Super-Sampling Anti-Aliasing
pub const SSAA: usize = 3;

#[cfg(feature = "ssaa-x4")]
/// Super-Sampling Anti-Aliasing
pub const SSAA: usize = 4;

#[cfg(not(any(feature = "text-ssaa-x2", feature = "text-ssaa-x4", feature = "text-ssaa-x6")))]
/// Text Super-Sampling Anti-Aliasing
pub const TEXT_SSAA: usize = 1;

#[cfg(feature = "text-ssaa-x2")]
/// Text Super-Sampling Anti-Aliasing
pub const TEXT_SSAA: usize = 2;

#[cfg(feature = "text-ssaa-x4")]
/// Text Super-Sampling Anti-Aliasing
pub const TEXT_SSAA: usize = 4;

#[cfg(feature = "text-ssaa-x6")]
/// Text Super-Sampling Anti-Aliasing
pub const TEXT_SSAA: usize = 6;
