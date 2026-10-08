// GET /api/config - what the checkout page needs, none of it secret: Paddle's client-side token is
// designed to be public, and price ids are visible in any checkout.
export default function handler(_req, res) {
  res.setHeader('Cache-Control', 'public, max-age=300');
  res.status(200).json({
    environment: process.env.PADDLE_ENV || 'sandbox',
    clientToken: process.env.PADDLE_CLIENT_TOKEN || '',
    prices: {
      monthly: process.env.PADDLE_PRICE_MONTHLY || '',
      yearly: process.env.PADDLE_PRICE_YEARLY || '',
    },
    download: process.env.DOWNLOAD_URL || '',
  });
}
