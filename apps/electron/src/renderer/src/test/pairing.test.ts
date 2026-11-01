import { describe, expect, it } from 'vitest';
import { qrText } from '../obid';

// docs/contracts/phone-pairing.md. What Settings › Runner's QR code carries: the Swift Mac app's (PairingView's qrText), which the
// iPhone decodes (GatewayEnvelope): the url, the link as an object with its relay, the secret
describe('the pairing QR code', () => {
  it('pairing_1: carries the url, the link with its relay, and the secret, as the iPhone reads them', () => {
    const relay = { sphere: 'https://sphere.example.com', id: 'r', token: 't', key: 'k' };
    const link = JSON.stringify({ hostname: 'studio', name: 'Studio', relay });
    const text = qrText({ url: 'https://sphere.example.com', link, secret: 's3cret', code: '123456', expiresAt: '2026-11-01T12:02:00Z' });
    expect(JSON.parse(text)).toEqual({ url: 'https://sphere.example.com', link: { hostname: 'studio', name: 'Studio', relay }, secret: 's3cret' });
    // never the six digits: through the Sphere only the QR's secret pairs
    expect(text).not.toContain('123456');
  });
});
