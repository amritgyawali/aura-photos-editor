// Which key, if any, is owed for a subscription or a just-completed checkout. ADR-0107.
import { day, issue } from './licence.js';
import { PaddleError } from './paddle.js';

const PAYING = new Set(['active', 'trialing', 'past_due']);
const SUBSCRIPTION = /^sub_[a-z0-9]{10,64}$/;
const TRANSACTION = /^txn_[a-z0-9]{10,64}$/;

/** A refusal with a status and a sentence for the customer. */
const refuse = (status, message) => new PaddleError(status, message);
const NO_MATCH = 'No subscription matches that number and email address. Check both against your receipt from Paddle.';

/**
 * The licence key owed for `subscription` (or the subscription a `transaction` created), if the
 * email matches the customer's. Keys run to the end of the paid period plus `graceDays`, so a
 * renewal payment has time to go through before anybody's export stops.
 */
export async function owedKey({ subscription, transaction, email }, { api, key, graceDays = 7, edition = 'pro' }) {
  const wanted = String(email || '').trim().toLowerCase();
  if (!wanted.includes('@')) throw refuse(400, 'Enter the email address you paid with.');
  let subscriptionId = subscription ? String(subscription).trim() : '';
  if (!subscriptionId) {
    const txn = String(transaction || '').trim();
    if (!TRANSACTION.test(txn)) throw refuse(400, 'Enter your subscription number (sub_...) or order number (txn_...).');
    const order = await api.transaction(txn);
    if (!order) throw refuse(404, NO_MATCH);
    if (!['paid', 'completed'].includes(order.status)) throw refuse(409, 'Your payment is still being processed. This page will try again in a moment.');
    if (!order.subscription_id) throw refuse(409, 'Your subscription is being set up. This page will try again in a moment.');
    subscriptionId = order.subscription_id;
  }
  if (!SUBSCRIPTION.test(subscriptionId)) throw refuse(400, 'A subscription number starts with sub_.');
  const sub = await api.subscription(subscriptionId);
  if (!sub) throw refuse(404, NO_MATCH);
  const customer = await api.customer(sub.customer_id);
  if (!customer || String(customer.email || '').trim().toLowerCase() !== wanted) throw refuse(404, NO_MATCH);
  if (!PAYING.has(sub.status)) {
    throw refuse(402, sub.status === 'canceled'
      ? 'This subscription was cancelled. Editing in AURA keeps working; subscribe again to export.'
      : 'This subscription is paused. Resume it from your Paddle receipt to export again.');
  }
  const periodEnd = sub.current_billing_period?.ends_at;
  if (!periodEnd) throw refuse(409, 'This subscription has no paid period yet. Try again in a moment.');
  const licence = {
    id: sub.id,
    name: (customer.name || '').trim() || customer.email,
    email: customer.email,
    edition,
    issued: day(sub.started_at || sub.created_at),
    expires: day(periodEnd, graceDays),
  };
  return { key: issue(licence, key), name: licence.name, email: licence.email, expires: licence.expires, subscription: sub.id };
}
