//! Frame-phase timing for the `profile` feature.

/// Times a block with hotpath under the `profile` feature, and is the block itself otherwise.
macro_rules! timed {
    ($label:literal, $body:expr) => {{
        #[cfg(feature = "profile")]
        {
            hotpath::measure_block!($label, $body)
        }
        #[cfg(not(feature = "profile"))]
        {
            $body
        }
    }};
}
