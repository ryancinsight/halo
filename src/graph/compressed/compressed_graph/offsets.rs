//! Run-length encoded offset compression.

/// Simple compressed offsets using run-length encoding for repeated values
#[derive(Clone, Debug)]
pub struct CompressedOffsets {
    /// Run-length encoded offset values
    values: Vec<usize>,
    /// Run lengths for each value
    runs: Vec<usize>,
}

impl CompressedOffsets {
    /// Create compressed offsets from uncompressed offsets
    pub fn from_offsets(offsets: &[usize]) -> Self {
        if offsets.is_empty() {
            return Self {
                values: Vec::new(),
                runs: Vec::new(),
            };
        }

        let mut values = Vec::new();
        let mut runs = Vec::new();

        let mut current_value = offsets[0];
        let mut current_run = 1;

        for &offset in &offsets[1..] {
            if offset == current_value {
                current_run += 1;
            } else {
                values.push(current_value);
                runs.push(current_run);
                current_value = offset;
                current_run = 1;
            }
        }

        // Add the last run
        values.push(current_value);
        runs.push(current_run);

        Self { values, runs }
    }

    /// Get offset at index
    #[inline]
    pub fn get(&self, index: usize) -> usize {
        let mut current_index = 0;
        for (&value, &run) in self.values.iter().zip(&self.runs) {
            if index < current_index + run {
                return value;
            }
            current_index += run;
        }
        0 // Default for out of bounds
    }

    /// Get the length of the original offsets array
    #[inline]
    pub fn len(&self) -> usize {
        self.runs.iter().sum()
    }

    /// Get the number of values in the compressed representation
    #[inline]
    pub fn values_len(&self) -> usize {
        self.values.len()
    }

    /// Get the number of runs in the compressed representation
    #[inline]
    pub fn runs_len(&self) -> usize {
        self.runs.len()
    }
}
