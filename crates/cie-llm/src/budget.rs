//! Budget (PRD §63): hard cap in USD-cents; on exceed the engine downgrades
//! to the offline provider and continues (acceptance #14).

use std::sync::atomic::{AtomicU64, Ordering};

pub struct Budget {
    used_cents: AtomicU64,
    limit_cents: u64,
}

impl Budget {
    pub fn new(limit_usd: f64) -> Budget {
        Budget {
            used_cents: AtomicU64::new(0),
            limit_cents: (limit_usd * 100.0) as u64,
        }
    }

    pub fn unlimited() -> Budget {
        Budget {
            used_cents: AtomicU64::new(0),
            limit_cents: u64::MAX,
        }
    }

    /// Record usage from (input_tokens, output_tokens) and the provider's
    /// per-1k pricing. Returns Err(BudgetExceeded) when over the cap.
    pub fn charge(
        &self,
        input_tokens: u64,
        output_tokens: u64,
        cost_per_1k: (f64, f64),
    ) -> Result<(), crate::LlmError> {
        let cents = ((input_tokens as f64 / 1000.0) * cost_per_1k.0
            + (output_tokens as f64 / 1000.0) * cost_per_1k.1) * 100.0;
        let prev = self.used_cents.fetch_add(cents.ceil() as u64, Ordering::SeqCst);
        if prev + (cents.ceil() as u64) > self.limit_cents {
            return Err(crate::LlmError::BudgetExceeded);
        }
        Ok(())
    }

    pub fn used_cents(&self) -> u64 {
        self.used_cents.load(Ordering::SeqCst)
    }

    pub fn exceeded(&self) -> bool {
        self.used_cents.load(Ordering::SeqCst) >= self.limit_cents
    }
}
