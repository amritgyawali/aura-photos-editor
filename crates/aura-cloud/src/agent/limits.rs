//! Four ceilings, checked before every step rather than after.
//!
//! An agent loop is the one place in this product where a model decides how much
//! work to do. Left alone it will occasionally decide to do an unbounded amount,
//! usually by cycling between two tools that each make the other look worth
//! trying again. So the loop asks [`LimitTracker::check`] before it does
//! anything, not after - a limit checked afterwards has already been exceeded,
//! and on the cost limit that means the user has already paid.
//!
//! The four are deliberately different kinds of ceiling, because they catch
//! different failures:
//!
//! | Limit | Catches |
//! |---|---|
//! | steps | a model cycling between tools |
//! | tokens | a scratchpad growing until the context window bursts |
//! | wall clock | a tool that is slow rather than wrong |
//! | cost | everything else, in the unit the user cares about |

use aura_core::errors::cloud::agent_limit;
use aura_core::AuraError;
use serde::{Deserialize, Serialize};

/// What one agent run may spend.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Limits {
    /// Tool calls plus reasoning turns.
    pub max_steps: u16,
    /// Prompt plus completion tokens across the whole run.
    pub max_tokens: u32,
    /// Wall clock for the whole run.
    pub max_wall_ms: u64,
    /// US dollars for the whole run.
    pub max_cost_usd: f64,
}

impl Default for Limits {
    /// Six steps, twenty thousand tokens, thirty seconds, five cents.
    ///
    /// Six steps is enough for "look at the evidence, ask one clarifying tool,
    /// answer" with room to spare, and small enough that a cycling model is
    /// stopped before it costs anything worth noticing. Five cents is two and a
    /// half times a single call's default ceiling, which is the point: an agent
    /// is allowed to be more expensive than one question, and not much more.
    fn default() -> Self {
        Self {
            max_steps: 6,
            max_tokens: 20_000,
            max_wall_ms: 30_000,
            max_cost_usd: 0.05,
        }
    }
}

/// What has been spent so far, and whether there is room for another step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitTracker {
    limits: Limits,
    steps: u16,
    tokens: u32,
    started_ms: u64,
    now_ms: u64,
    cost_usd: f64,
}

/// Which ceiling was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exceeded {
    /// Too many tool calls or turns.
    Steps,
    /// Too many tokens across the run.
    Tokens,
    /// Too long on the clock.
    WallClock,
    /// Too much money.
    Cost,
}

impl Exceeded {
    /// Stable text for the error context and the audit row.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Steps => "steps",
            Self::Tokens => "tokens",
            Self::WallClock => "wall_clock",
            Self::Cost => "cost",
        }
    }
}

impl LimitTracker {
    /// Start tracking, from the clock's current monotonic reading.
    #[must_use]
    pub const fn start(limits: Limits, now_ms: u64) -> Self {
        Self {
            limits,
            steps: 0,
            tokens: 0,
            started_ms: now_ms,
            now_ms,
            cost_usd: 0.0,
        }
    }

    /// Tell the tracker what time it is. The loop does this before each check,
    /// so the tracker never reads a clock itself and stays testable.
    pub fn observe(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// Record what one step cost.
    pub fn charge(&mut self, tokens: u32, cost_usd: f64) {
        self.steps = self.steps.saturating_add(1);
        self.tokens = self.tokens.saturating_add(tokens);
        self.cost_usd += cost_usd.max(0.0);
    }

    /// Steps taken.
    #[must_use]
    pub const fn steps(&self) -> u16 {
        self.steps
    }

    /// Tokens spent.
    #[must_use]
    pub const fn tokens(&self) -> u32 {
        self.tokens
    }

    /// Money spent.
    #[must_use]
    pub const fn cost_usd(&self) -> f64 {
        self.cost_usd
    }

    /// Milliseconds elapsed.
    #[must_use]
    pub const fn elapsed_ms(&self) -> u64 {
        self.now_ms.saturating_sub(self.started_ms)
    }

    /// Which ceiling, if any, is already reached.
    #[must_use]
    pub fn exceeded(&self) -> Option<Exceeded> {
        if self.steps >= self.limits.max_steps {
            return Some(Exceeded::Steps);
        }
        if self.tokens >= self.limits.max_tokens {
            return Some(Exceeded::Tokens);
        }
        if self.elapsed_ms() >= self.limits.max_wall_ms {
            return Some(Exceeded::WallClock);
        }
        if self.cost_usd >= self.limits.max_cost_usd {
            return Some(Exceeded::Cost);
        }
        None
    }

    /// Room for another step, or the error explaining why not.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6011`, naming the ceiling and what was spent.
    pub fn check(&self) -> Result<(), AuraError> {
        match self.exceeded() {
            None => Ok(()),
            Some(which) => Err(agent_limit(
                which.as_str(),
                format!(
                    "stopped at {} steps, {} tokens, {} ms, {:.4} USD",
                    self.steps,
                    self.tokens,
                    self.elapsed_ms(),
                    self.cost_usd
                ),
            )),
        }
    }
}
