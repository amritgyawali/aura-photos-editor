// The few Paddle Billing API reads the licence server needs. Read-only: the shop never changes an
// order, it only looks one up to decide whether a key is owed.
const BASES = { sandbox: 'https://sandbox-api.paddle.com', live: 'https://api.paddle.com' };

export class PaddleError extends Error {
  constructor(status, message) { super(message); this.status = status; }
}

export function paddle({ apiKey = process.env.PADDLE_API_KEY, environment = process.env.PADDLE_ENV || 'sandbox', fetchImpl = fetch } = {}) {
  const base = BASES[environment];
  if (!base) throw new Error(`PADDLE_ENV must be sandbox or live, not ${environment}`);
  if (!apiKey) throw new Error('PADDLE_API_KEY is not set');
  const get = async (path) => {
    const response = await fetchImpl(`${base}${path}`, { headers: { Authorization: `Bearer ${apiKey}`, Accept: 'application/json' } });
    if (response.status === 404) return null;
    if (!response.ok) throw new PaddleError(502, `Paddle answered ${response.status} for ${path.split('/')[1]}`);
    return (await response.json()).data;
  };
  return {
    transaction: (id) => get(`/transactions/${encodeURIComponent(id)}`),
    subscription: (id) => get(`/subscriptions/${encodeURIComponent(id)}`),
    customer: (id) => get(`/customers/${encodeURIComponent(id)}`),
  };
}
