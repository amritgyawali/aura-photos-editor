// Checkout: Paddle's overlay, then straight to the key page for the order just paid.
(async () => {
  const status = document.getElementById('shop-status');
  let config;
  try {
    config = await (await fetch('/api/config')).json();
  } catch {
    status.textContent = 'The shop could not load. Please try again in a minute.';
    return;
  }
  const download = document.getElementById('download');
  if (download && config.download) download.href = config.download;
  if (!config.clientToken || !window.Paddle) {
    status.textContent = 'Checkout is not open yet.';
    return;
  }
  if (config.environment === 'sandbox') {
    window.Paddle.Environment.set('sandbox');
    status.textContent = 'Test mode: no real payment is taken.';
  }
  window.Paddle.Initialize({
    token: config.clientToken,
    eventCallback(event) {
      if (event.name !== 'checkout.completed') return;
      const transaction = event.data?.transaction_id;
      const email = event.data?.customer?.email;
      if (transaction && email) {
        window.Paddle.Checkout.close();
        location.href = `/key?transaction=${encodeURIComponent(transaction)}&email=${encodeURIComponent(email)}`;
      }
    },
  });
  for (const button of document.querySelectorAll('button[data-plan]')) {
    const priceId = config.prices?.[button.dataset.plan];
    if (!priceId) continue;
    button.disabled = false;
    button.addEventListener('click', () => {
      window.Paddle.Checkout.open({ items: [{ priceId, quantity: 1 }] });
    });
  }
})();
