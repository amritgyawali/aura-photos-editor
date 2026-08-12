//! The cost governor: the user's money, spent on purpose.
//!
//! The rule that makes this module worth having is that **the estimate happens
//! before the call**. A governor that added up what was spent afterwards is an
//! accountant, not a governor - by the time it notices, the money is gone. So
//! every call is priced from its token and image counts against the chosen
//! model's row, and compared to three ceilings in a fixed order:
//!
//! 1. The task's own `max_cost_usd`. A single call that costs ten times what its
//!    kind of decision is worth is a bug, whatever the budget says.
//! 2. What is left of the project cap.
//! 3. What is left of the month cap.
//!
//! Failing the first is `AURA-CLOUD-6014`; failing either of the others is
//! `AURA-CLOUD-6006`. Both fall back locally and the wedding finishes.
//!
//! ## Downgrade before refusal
//!
//! Section 6.3: "downgrade to `balanced` tier when budget < 30 % remains". The
//! governor tries the cheapest acceptable tier, and only refuses when even that
//! is over a ceiling. A slightly worse answer for the last fifth of a wedding is
//! a much better outcome than no answer, and the audit row records which
//! decisions were made on a downgraded tier so the Explain panel can say so.
//!
//! ## The price table is versioned data
//!
//! Prices change. Every audit row records [`PRICE_TABLE_VERSION`], so a bill that
//! does not reconcile can be traced to the table that priced it rather than
//! argued about. The shipped numbers are defaults; an installation may override
//! them, and the version is bumped whenever the shipped ones move.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::errors::cloud::{budget_reached, cost_ceiling};
use aura_core::errors::db::statement_failed;
use aura_core::AuraResult;
use rusqlite::params;

use crate::contract::cloud::Tier;
use crate::provider::{estimate_tokens_in, ModelAlias, ProviderConfig};

/// Bumped whenever the shipped price tables change. Recorded on every call.
pub const PRICE_TABLE_VERSION: u32 = 1;

/// The reserved project id the monthly cap is filed under.
pub const MONTH_SCOPE: &str = "__month__";

/// Below this fraction of the cap remaining, the governor drops a tier.
pub const DOWNGRADE_THRESHOLD: f64 = 0.30;

/// The default per-project cap: section 11's cost budget for a 3,000-image
/// wedding, which is also what the first-run wizard offers.
pub const DEFAULT_PROJECT_CAP_USD: f64 = 1.50;

/// The default monthly cap. Twenty weddings' worth, which is a busy season.
pub const DEFAULT_MONTH_CAP_USD: f64 = 30.00;

/// One cap and what has been spent against it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetState {
    /// The ceiling, in US dollars.
    pub cap_usd: f64,
    /// Billed so far.
    pub spent_usd: f64,
    /// Calls made.
    pub calls: u64,
    /// Calls made on a tier below the task's preference.
    pub downgrades: u64,
    /// When false, passing the cap logs and continues rather than stopping.
    pub hard_stop: bool,
}

impl Default for BudgetState {
    fn default() -> Self {
        Self {
            cap_usd: DEFAULT_PROJECT_CAP_USD,
            spent_usd: 0.0,
            calls: 0,
            downgrades: 0,
            hard_stop: true,
        }
    }
}

impl BudgetState {
    /// What is left, never below zero.
    #[must_use]
    pub fn remaining_usd(&self) -> f64 {
        (self.cap_usd - self.spent_usd).max(0.0)
    }

    /// Fraction of the cap still available, 0.0 to 1.0.
    #[must_use]
    pub fn remaining_fraction(&self) -> f64 {
        if self.cap_usd <= 0.0 {
            return 0.0;
        }
        (self.remaining_usd() / self.cap_usd).clamp(0.0, 1.0)
    }

    /// True when the governor should start choosing cheaper models.
    #[must_use]
    pub fn should_downgrade(&self) -> bool {
        self.remaining_fraction() < DOWNGRADE_THRESHOLD
    }

    /// True when no further billed call may be made.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.hard_stop && self.remaining_usd() <= 0.0
    }
}

/// Reading and updating the caps.
pub trait BudgetStore: Send + Sync + std::fmt::Debug {
    /// The state of one scope: a project id, or [`MONTH_SCOPE`].
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the store cannot be read.
    fn state(&self, scope: &str) -> AuraResult<BudgetState>;

    /// Set the cap for one scope.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the store cannot be written.
    fn set_cap(&self, scope: &str, cap_usd: f64, hard_stop: bool) -> AuraResult<()>;

    /// Add one billed call's cost.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the store cannot be written.
    fn charge(&self, scope: &str, cost_usd: f64, downgraded: bool) -> AuraResult<()>;

    /// Record that the cap stopped further calls, for the report.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the store cannot be written.
    fn note_stop(&self, scope: &str) -> AuraResult<()>;
}

/// What the governor decided about one prospective call.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The tier that will be used.
    pub tier: Tier,
    /// The model that will be asked.
    pub model: ModelAlias,
    /// Estimated prompt tokens.
    pub tokens_in: u32,
    /// Estimated cost of the whole call, in US dollars.
    pub estimate_usd: f64,
    /// True when the chosen tier is below the task's declared minimum preference.
    pub downgraded: bool,
}

/// The governor itself: pure decision-making over a store's numbers.
#[derive(Debug)]
pub struct CostGovernor {
    store: Arc<dyn BudgetStore>,
}

impl CostGovernor {
    /// A governor over one store.
    #[must_use]
    pub fn new(store: Arc<dyn BudgetStore>) -> Self {
        Self { store }
    }

    /// The store behind it, for the settings panel.
    #[must_use]
    pub fn store(&self) -> &Arc<dyn BudgetStore> {
        &self.store
    }

    /// Choose a tier and price the call, or refuse it.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6006` when a cap is reached, `AURA-CLOUD-6014` when even the
    /// cheapest acceptable tier costs more than the task allows.
    pub fn plan(
        &self,
        task: &str,
        project_id: &str,
        config: &ProviderConfig,
        prompt: &crate::contract::cloud::PromptSpec,
        max_cost_usd: f64,
    ) -> AuraResult<Plan> {
        let project = self.store.state(project_id)?;
        let month = self.store.state(MONTH_SCOPE)?;

        if project.is_stopped() {
            self.store.note_stop(project_id)?;
            return Err(budget_reached(
                "this job",
                project.cap_usd,
                project.spent_usd,
            ));
        }
        if month.is_stopped() {
            self.store.note_stop(MONTH_SCOPE)?;
            return Err(budget_reached("this month", month.cap_usd, month.spent_usd));
        }

        // Start at the task's declared minimum, or one step below it when either
        // budget is running low. `min_tier` is the *cheapest acceptable* tier, so
        // "downgrading" below it is only done when the alternative is refusing.
        let wanted = prompt.min_tier;
        let mut tier = wanted;
        let mut downgraded = false;
        if project.should_downgrade() || month.should_downgrade() {
            if let Some(cheaper) = wanted.cheaper() {
                tier = cheaper;
                downgraded = true;
            }
        }

        // Walk down the tiers until one is affordable.
        loop {
            let Some(alias) = config.alias(tier) else {
                return Err(cost_ceiling(task, 0.0, max_cost_usd));
            };
            let tokens_in = estimate_tokens_in(prompt, alias);
            let estimate = alias.price(tokens_in, prompt.max_tokens);

            let affordable = estimate <= max_cost_usd
                && estimate <= project.remaining_usd()
                && estimate <= month.remaining_usd();

            if affordable {
                return Ok(Plan {
                    tier,
                    model: alias.clone(),
                    tokens_in,
                    estimate_usd: estimate,
                    downgraded: downgraded || tier < wanted,
                });
            }

            let Some(cheaper) = tier.cheaper() else {
                // Nothing left to try. Which ceiling was hit decides the code,
                // because "you set a small cap" and "this task is priced wrong"
                // are different conversations.
                if estimate > max_cost_usd {
                    return Err(cost_ceiling(task, estimate, max_cost_usd));
                }
                let (scope, state) = if estimate > project.remaining_usd() {
                    ("this job", project)
                } else {
                    ("this month", month)
                };
                return Err(budget_reached(scope, state.cap_usd, state.spent_usd));
            };
            tier = cheaper;
            downgraded = true;
        }
    }

    /// Record what a completed call actually cost, against both scopes.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the store cannot be written.
    pub fn charge(&self, project_id: &str, cost_usd: f64, downgraded: bool) -> AuraResult<()> {
        self.store.charge(project_id, cost_usd, downgraded)?;
        self.store.charge(MONTH_SCOPE, cost_usd, downgraded)
    }
}

/// Caps in the project's catalog.
#[derive(Debug)]
pub struct CatalogBudget {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl CatalogBudget {
    /// Use one catalog's `cloud_budget` table.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// The current month as `YYYY-MM`, from the injected clock.
    fn month(&self) -> String {
        let now = self.clock.now_utc();
        format!("{:04}-{:02}", now.year(), u8::from(now.month()))
    }
}

impl BudgetStore for CatalogBudget {
    fn state(&self, scope: &str) -> AuraResult<BudgetState> {
        let key = scope.to_string();
        let this_month = self.month();
        let default_cap = if scope == MONTH_SCOPE {
            DEFAULT_MONTH_CAP_USD
        } else {
            DEFAULT_PROJECT_CAP_USD
        };

        self.catalog.read(move |conn| {
            let found = conn.query_row(
                "SELECT cap_usd, spent_usd, month, hard_stop, calls, downgrades
                 FROM cloud_budget WHERE project_id = ?1",
                params![key],
                |row| {
                    Ok((
                        row.get::<_, Option<f64>>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            );

            match found {
                Ok((cap, spent, month, hard_stop, calls, downgrades)) => {
                    // A monthly row from a previous month is a spent figure that
                    // no longer applies. It is treated as zero here rather than
                    // being reset by a write, so a read never mutates and a
                    // machine whose clock jumps backwards does not lose history.
                    let stale = month.is_some_and(|stored| stored != this_month);
                    Ok(BudgetState {
                        cap_usd: cap.unwrap_or(default_cap),
                        spent_usd: if stale { 0.0 } else { spent },
                        calls: u64::try_from(calls).unwrap_or(0),
                        downgrades: u64::try_from(downgrades).unwrap_or(0),
                        hard_stop: hard_stop != 0,
                    })
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(BudgetState {
                    cap_usd: default_cap,
                    ..BudgetState::default()
                }),
                Err(err) => Err(statement_failed("cloud budget read failed", &err)),
            }
        })
    }

    fn set_cap(&self, scope: &str, cap_usd: f64, hard_stop: bool) -> AuraResult<()> {
        let key = scope.to_string();
        let month = self.month();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let cap = cap_usd.max(0.0);
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "INSERT INTO cloud_budget (project_id, cap_usd, spent_usd, month, hard_stop, updated_at)
                 VALUES (?1, ?2, 0, ?3, ?4, ?5)
                 ON CONFLICT(project_id) DO UPDATE SET
                   cap_usd = excluded.cap_usd,
                   hard_stop = excluded.hard_stop,
                   updated_at = excluded.updated_at",
                params![key, cap, month, i64::from(hard_stop), now],
            )
            .map(|_| ())
            .map_err(|err| statement_failed("cloud budget write failed", &err))
        })
    }

    fn charge(&self, scope: &str, cost_usd: f64, downgraded: bool) -> AuraResult<()> {
        let key = scope.to_string();
        let month = self.month();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let cost = cost_usd.max(0.0);
        let default_cap = if scope == MONTH_SCOPE {
            DEFAULT_MONTH_CAP_USD
        } else {
            DEFAULT_PROJECT_CAP_USD
        };

        self.catalog.writer().with(move |conn| {
            // A month boundary resets `spent_usd` as part of the same statement
            // that adds to it, so there is no window in which a new month is
            // charged against last month's total.
            conn.execute(
                "INSERT INTO cloud_budget
                   (project_id, cap_usd, spent_usd, month, hard_stop, calls, downgrades, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, 1, ?5, ?6)
                 ON CONFLICT(project_id) DO UPDATE SET
                   spent_usd  = CASE WHEN cloud_budget.month = excluded.month
                                     THEN cloud_budget.spent_usd + excluded.spent_usd
                                     ELSE excluded.spent_usd END,
                   calls      = CASE WHEN cloud_budget.month = excluded.month
                                     THEN cloud_budget.calls + 1 ELSE 1 END,
                   downgrades = CASE WHEN cloud_budget.month = excluded.month
                                     THEN cloud_budget.downgrades + excluded.downgrades
                                     ELSE excluded.downgrades END,
                   month      = excluded.month,
                   updated_at = excluded.updated_at",
                params![key, default_cap, cost, month, i64::from(downgraded), now],
            )
            .map(|_| ())
            .map_err(|err| statement_failed("cloud budget charge failed", &err))
        })
    }

    fn note_stop(&self, scope: &str) -> AuraResult<()> {
        let key = scope.to_string();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        self.catalog.writer().with(move |conn| {
            conn.execute(
                "UPDATE cloud_budget SET stopped_at = ?2 WHERE project_id = ?1 AND stopped_at IS NULL",
                params![key, now],
            )
            .map(|_| ())
            .map_err(|err| statement_failed("cloud budget stop marker failed", &err))
        })
    }
}

/// Caps in memory. The gate and the unit tests use it.
#[derive(Debug, Default)]
pub struct MemoryBudget {
    states: parking_lot::Mutex<std::collections::BTreeMap<String, BudgetState>>,
}

impl MemoryBudget {
    /// Everything at the defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// One scope pre-set to a cap.
    #[must_use]
    pub fn with_cap(scope: &str, cap_usd: f64) -> Self {
        let store = Self::new();
        store.states.lock().insert(
            scope.to_string(),
            BudgetState {
                cap_usd,
                ..BudgetState::default()
            },
        );
        store
    }
}

impl BudgetStore for MemoryBudget {
    fn state(&self, scope: &str) -> AuraResult<BudgetState> {
        Ok(self
            .states
            .lock()
            .get(scope)
            .copied()
            .unwrap_or(BudgetState {
                cap_usd: if scope == MONTH_SCOPE {
                    DEFAULT_MONTH_CAP_USD
                } else {
                    DEFAULT_PROJECT_CAP_USD
                },
                ..BudgetState::default()
            }))
    }

    fn set_cap(&self, scope: &str, cap_usd: f64, hard_stop: bool) -> AuraResult<()> {
        let mut states = self.states.lock();
        let entry = states.entry(scope.to_string()).or_default();
        entry.cap_usd = cap_usd.max(0.0);
        entry.hard_stop = hard_stop;
        Ok(())
    }

    fn charge(&self, scope: &str, cost_usd: f64, downgraded: bool) -> AuraResult<()> {
        let mut states = self.states.lock();
        let default_cap = if scope == MONTH_SCOPE {
            DEFAULT_MONTH_CAP_USD
        } else {
            DEFAULT_PROJECT_CAP_USD
        };
        let entry = states.entry(scope.to_string()).or_insert(BudgetState {
            cap_usd: default_cap,
            ..BudgetState::default()
        });
        entry.spent_usd += cost_usd.max(0.0);
        entry.calls += 1;
        if downgraded {
            entry.downgrades += 1;
        }
        Ok(())
    }

    fn note_stop(&self, _scope: &str) -> AuraResult<()> {
        Ok(())
    }
}
