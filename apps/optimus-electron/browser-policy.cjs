function assertPreviewUrl(input) {
  let url;
  try {
    url = new URL(input);
  } catch {
    throw new Error('Malformed preview URL');
  }
  if (url.protocol === 'https:') return url.toString();
  if (
    url.protocol === 'http:' &&
    (url.hostname === '127.0.0.1' || url.hostname === 'localhost' || url.hostname === '::1')
  ) {
    return url.toString();
  }
  throw new Error(
    `Preview permits HTTPS and loopback HTTP only (received ${url.protocol}//${url.hostname})`
  );
}

module.exports = { assertPreviewUrl };
