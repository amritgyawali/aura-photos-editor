//! The single-writer actor. Exactly one thread ever holds a writable connection.

use std::path::Path;
use std::thread::{Builder, JoinHandle};

use aura_core::errors::db::{open_failed_with, statement_failed, writer_gone};
use aura_core::AuraResult;
use crossbeam_channel::{bounded, Receiver, Sender};
use rusqlite::{Connection, Transaction, TransactionBehavior};

/// A unit of work executed on the writer thread. Returning a `Result` through a
/// oneshot channel keeps the caller's error handling completely ordinary.
type WriteJob = Box<dyn FnOnce(&mut Connection) + Send>;

/// A cloneable handle to the one writer thread of a catalog.
#[derive(Debug, Clone)]
pub struct Writer {
    tx: Sender<WriteJob>,
}

impl Writer {
    /// Spawn the one and only writer thread for this catalog.
    ///
    /// # Errors
    ///
    /// Returns `AURA-DB-3001` when the connection or the thread cannot be created.
    pub fn spawn(path: &Path) -> AuraResult<(Self, JoinHandle<()>)> {
        let (tx, rx): (Sender<WriteJob>, Receiver<WriteJob>) = bounded(256);
        let mut conn = crate::db::open_raw(path, true)?;
        let handle = Builder::new()
            .name("aura-catalog-writer".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    job(&mut conn);
                }
            })
            .map_err(|e| open_failed_with("could not start the catalog writer thread", &e))?;
        Ok((Self { tx }, handle))
    }

    /// Run a closure on the writer thread and wait for its result.
    ///
    /// # Errors
    ///
    /// Propagates the closure's error, or `AURA-DB-3006` when the writer thread is gone.
    pub fn with<T, F>(&self, f: F) -> AuraResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> AuraResult<T> + Send + 'static,
    {
        let (rtx, rrx) = bounded(1);
        self.tx
            .send(Box::new(move |conn| {
                let _ = rtx.send(f(conn));
            }))
            .map_err(|_| writer_gone())?;
        rrx.recv().map_err(|_| writer_gone())?
    }

    /// Run a closure inside an IMMEDIATE transaction. Any error rolls back.
    ///
    /// `TransactionBehavior::Immediate` takes the write lock at BEGIN rather than at
    /// the first write, which turns a late `SQLITE_BUSY` into an instant, cheap wait.
    ///
    /// # Errors
    ///
    /// Propagates the closure's error, or `AURA-DB-3006` when begin or commit fails.
    pub fn transact<T, F>(&self, f: F) -> AuraResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> AuraResult<T> + Send + 'static,
    {
        self.with(move |conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|e| statement_failed("could not begin a write transaction", &e))?;
            let out = f(&tx)?;
            tx.commit()
                .map_err(|e| statement_failed("could not commit a write transaction", &e))?;
            Ok(out)
        })
    }
}
