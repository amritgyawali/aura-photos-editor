// GET /api/licence?subscription=sub_...&email=...   (the app's renewal, and "Get my key")
// GET /api/licence?transaction=txn_...&email=...    (right after checkout)
//
// Answers { key, name, email, expires, subscription } or { error } with a status. Stores nothing.
import { vendorKey } from '../lib/licence.js';
import { owedKey } from '../lib/owed.js';
import { paddle, PaddleError } from '../lib/paddle.js';

export default async function handler(req, res) {
  res.setHeader('Cache-Control', 'no-store');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Use GET.' });
    return;
  }
  try {
    const { subscription, transaction, email } = req.query;
    const result = await owedKey({ subscription, transaction, email }, {
      api: paddle(),
      key: vendorKey(process.env.AURA_VENDOR_KEY),
      graceDays: Number(process.env.LICENCE_GRACE_DAYS || 7),
      edition: process.env.AURA_EDITION || 'pro',
    });
    res.status(200).json(result);
  } catch (error) {
    if (error instanceof PaddleError) {
      res.status(error.status).json({ error: error.message });
    } else {
      console.error(error);
      res.status(500).json({ error: 'The licence server is not set up correctly. Please email support.' });
    }
  }
}
