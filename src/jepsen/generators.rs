//! Test data generators for Jepsen tests

use rand::Rng;
use serde_json::Value;

/// Generate random test data
pub struct DataGenerator {
    rng: Box<dyn rand::RngCore + Send>,
}

impl DataGenerator {
    pub fn new() -> Self {
        use rand::SeedableRng;
        Self {
            rng: Box::new(rand::rngs::StdRng::from_entropy()),
        }
    }

    pub fn with_seed(seed: u64) -> Self {
        use rand::SeedableRng;
        Self {
            rng: Box::new(rand::rngs::StdRng::seed_from_u64(seed)),
        }
    }

    /// Generate random JSON value
    pub fn random_value(&mut self) -> Value {
        match self.rng.gen_range(0..6) {
            0 => Value::Null,
            1 => Value::Bool(self.rng.gen()),
            2 => Value::Number(self.rng.gen::<u64>().into()),
            3 => Value::Number(self.rng.gen::<i64>().into()),
            4 => Value::Number(serde_json::Number::from_f64(self.rng.gen::<f64>()).unwrap()),
            _ => Value::String(self.random_string(10)),
        }
    }

    /// Generate random string
    pub fn random_string(&mut self, length: usize) -> String {
        const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        (0..length)
            .map(|_| {
                let idx = self.rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// Generate random key from a set
    pub fn random_key(&mut self, keys: &[String]) -> String {
        keys[self.rng.gen_range(0..keys.len())].clone()
    }

    /// Generate random integer in range
    pub fn random_int(&mut self, min: i64, max: i64) -> i64 {
        self.rng.gen_range(min..=max)
    }

    /// Generate random float in range
    pub fn random_float(&mut self, min: f64, max: f64) -> f64 {
        self.rng.gen_range(min..=max)
    }
}

/// Generate test keys
pub fn generate_keys(prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|i| format!("{}-{}", prefix, i)).collect()
}

/// Generate test accounts for bank workload
pub fn generate_accounts(count: usize, initial_balance: u64) -> Vec<(String, u64)> {
    (0..count)
        .map(|i| (format!("account-{}", i), initial_balance))
        .collect()
}

/// Generate test data for different workload types
pub mod workload_data {
    use super::*;

    /// Generate register test data
    pub fn register_data(num_keys: usize) -> Vec<String> {
        generate_keys("register", num_keys)
    }

    /// Generate set test data
    pub fn set_data(num_sets: usize, elements_per_set: usize) -> (Vec<String>, Vec<Value>) {
        let keys = generate_keys("set", num_sets);
        let elements = (0..elements_per_set)
            .map(|i| Value::Number(i.into()))
            .collect();
        (keys, elements)
    }

    /// Generate bank test data
    pub fn bank_data(num_accounts: usize, initial_balance: u64) -> Vec<(String, u64)> {
        generate_accounts(num_accounts, initial_balance)
    }

    /// Generate counter test data
    pub fn counter_data(num_counters: usize) -> Vec<String> {
        generate_keys("counter", num_counters)
    }

    /// Generate list test data
    pub fn list_data(num_lists: usize) -> Vec<String> {
        generate_keys("list", num_lists)
    }
}

/// Probability distributions for operation generation
pub mod distributions {
    use rand_distr::{Distribution, Exp, Normal, Uniform};

    /// Exponential distribution for inter-arrival times
    pub struct ExponentialGenerator {
        dist: Exp<f64>,
    }

    impl ExponentialGenerator {
        pub fn new(rate: f64) -> Self {
            Self {
                dist: Exp::new(rate).unwrap(),
            }
        }

        pub fn sample(&self, rng: &mut dyn rand::RngCore) -> f64 {
            self.dist.sample(rng)
        }
    }

    /// Normal distribution for latency simulation
    pub struct NormalGenerator {
        dist: Normal<f64>,
    }

    impl NormalGenerator {
        pub fn new(mean: f64, std_dev: f64) -> Self {
            Self {
                dist: Normal::new(mean, std_dev).unwrap(),
            }
        }

        pub fn sample(&self, rng: &mut dyn rand::RngCore) -> f64 {
            self.dist.sample(rng).max(0.0) // Ensure non-negative
        }
    }

    /// Uniform distribution for random selection
    pub struct UniformGenerator {
        dist: Uniform<f64>,
    }

    impl UniformGenerator {
        pub fn new(min: f64, max: f64) -> Self {
            Self {
                dist: Uniform::new(min, max),
            }
        }

        pub fn sample(&self, rng: &mut dyn rand::RngCore) -> f64 {
            self.dist.sample(rng)
        }
    }
}

/// Load patterns for testing
pub mod load_patterns {
    use std::time::Duration;

    /// Constant load pattern
    pub struct ConstantLoad {
        pub rate: f64,
    }

    impl ConstantLoad {
        pub fn new(rate: f64) -> Self {
            Self { rate }
        }

        pub fn next_interval(&self) -> Duration {
            Duration::from_secs_f64(1.0 / self.rate)
        }
    }

    /// Bursty load pattern
    pub struct BurstyLoad {
        pub base_rate: f64,
        pub burst_rate: f64,
        pub burst_duration: Duration,
        pub burst_interval: Duration,
        current_time: Duration,
        last_burst: Duration,
    }

    impl BurstyLoad {
        pub fn new(
            base_rate: f64,
            burst_rate: f64,
            burst_duration: Duration,
            burst_interval: Duration,
        ) -> Self {
            Self {
                base_rate,
                burst_rate,
                burst_duration,
                burst_interval,
                current_time: Duration::ZERO,
                last_burst: Duration::ZERO,
            }
        }

        pub fn next_interval(&mut self) -> Duration {
            self.current_time += Duration::from_millis(1);

            let time_since_burst = self.current_time - self.last_burst;
            let in_burst = time_since_burst < self.burst_duration;
            let should_burst = time_since_burst >= self.burst_interval;

            if should_burst && !in_burst {
                self.last_burst = self.current_time;
            }

            let rate = if in_burst || should_burst {
                self.burst_rate
            } else {
                self.base_rate
            };

            Duration::from_secs_f64(1.0 / rate)
        }
    }

    /// Ramp load pattern (gradually increasing)
    pub struct RampLoad {
        pub start_rate: f64,
        pub end_rate: f64,
        pub duration: Duration,
        start_time: std::time::Instant,
    }

    impl RampLoad {
        pub fn new(start_rate: f64, end_rate: f64, duration: Duration) -> Self {
            Self {
                start_rate,
                end_rate,
                duration,
                start_time: std::time::Instant::now(),
            }
        }

        pub fn next_interval(&self) -> Duration {
            let elapsed = self.start_time.elapsed();
            let progress = (elapsed.as_secs_f64() / self.duration.as_secs_f64()).min(1.0);
            
            let current_rate = self.start_rate + (self.end_rate - self.start_rate) * progress;
            Duration::from_secs_f64(1.0 / current_rate)
        }
    }
}