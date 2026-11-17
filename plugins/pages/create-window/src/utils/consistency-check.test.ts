import { describe, expect, it } from 'vitest';
import type { WindowConfig } from '../types';
import { applyAllFixes, checkConsistency } from './consistency-check';

const WIN_UA = (major: number) =>
  `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/${major}.0.0.0 Safari/537.36`;

function config(overrides: {
  windowInfo?: Partial<WindowConfig['windowInfo']>;
  basicSettings?: Partial<WindowConfig['basicSettings']>;
  advancedFingerprintSettings?: Partial<WindowConfig['advancedFingerprintSettings']>;
  deviceSettings?: Partial<WindowConfig['deviceSettings']>;
} = {}): WindowConfig {
  const base = {
    windowInfo: {
      name: '',
      system: 'Windows',
      kernel: 'Chrome 158',
      kernelId: 'k',
      userAgent: WIN_UA(158),
      searchEngine: 'google',
      proxySourceMode: 'remote',
      proxyUuids: ['p1'],
      localProxyNodeNames: [],
      accountUuids: [],
      urls: [],
      cookies: [],
      description: '',
    },
    basicSettings: {
      language: 'ip',
      interfaceLanguage: 'ip',
      timezone: 'ip',
      geolocationPrompt: 'ask',
      geolocation: 'ip',
      sound: true,
      images: true,
      video: true,
      windowSize: 'fullscreen',
      windowPosition: 'top-left',
    },
    advancedFingerprintSettings: {
      resolution: { width: 1920, height: 1080 },
      colorDepth: 24,
      devicePixelRatio: 1,
      maxTouchPoints: 0,
      fontFingerprint: 'random',
      fontList: { mode: 'random' },
      webrtc: 'replace',
      webglImage: 'random',
      webglInfo: 'custom',
      webglVendor: 'Google Inc. (NVIDIA)',
      webglRenderer:
        'ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 (0x00002504) Direct3D11 vs_5_0 ps_5_0, D3D11)',
      webgpu: 'webgl-match',
      canvas: 'random',
      audioContext: 'random',
      speechVoices: 'random',
      doNotTrack: false,
      clientRects: 'random',
      mediaDevices: 'random',
    },
    deviceSettings: {
      deviceName: 'DESKTOP-1',
      deviceNameRandom: false,
      macAddress: '',
      macAddressMode: 'real',
      hardwareConcurrency: 8,
      deviceMemory: 8,
      sslFingerprint: false,
      portScanProtection: true,
      scanWhitelist: '',
      hardwareAcceleration: true,
      disableSandbox: false,
      startupParameters: '',
    },
    preferenceSettings: {},
    projectMetadata: { defaultProject: '', tags: [] },
  } as unknown as WindowConfig;
  return {
    ...base,
    windowInfo: { ...base.windowInfo, ...overrides.windowInfo },
    basicSettings: { ...base.basicSettings, ...overrides.basicSettings },
    advancedFingerprintSettings: {
      ...base.advancedFingerprintSettings,
      ...overrides.advancedFingerprintSettings,
    },
    deviceSettings: { ...base.deviceSettings, ...overrides.deviceSettings },
  };
}

const ids = (c: WindowConfig) => checkConsistency(c).map((issue) => issue.id);

describe('checkConsistency', () => {
  it('reports nothing for a consistent Windows profile', () => {
    expect(ids(config())).toEqual([]);
  });

  it('flags a UA major version that differs from the kernel and fixes it in place', () => {
    const edge = `${WIN_UA(140)} Edg/140.0.0.0`;
    const c = config({ windowInfo: { userAgent: edge } });
    expect(ids(c)).toEqual(['uaVersionMismatch']);
    const fixed = applyAllFixes(c);
    expect(fixed.windowInfo.userAgent).toBe(`${WIN_UA(158)} Edg/158.0.0.0`);
    expect(ids(fixed)).toEqual([]);
  });

  it('flags a Safari or Firefox UA on the Chromium kernel', () => {
    const safari =
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15';
    const c = config({ windowInfo: { userAgent: safari } });
    expect(ids(c)).toContain('uaBrowserMismatch');
    expect(applyAllFixes(c).windowInfo.userAgent).toContain('Chrome/158.0.0.0');
  });

  it('flags a UA whose system differs from the selected system', () => {
    const mac =
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/158.0.0.0 Safari/537.36';
    expect(ids(config({ windowInfo: { userAgent: mac } }))).toContain('uaSystemMismatch');
  });

  it('flags an Apple GPU on Windows', () => {
    const c = config({
      advancedFingerprintSettings: {
        webglRenderer: 'ANGLE (Apple, ANGLE Metal Renderer: Apple M2, Unspecified Version)',
      },
    });
    expect(ids(c)).toEqual(['webglSystemMismatch']);
  });

  it('fixes impossible hardware values', () => {
    const c = config({
      deviceSettings: { deviceMemory: 16, hardwareConcurrency: 7 },
      advancedFingerprintSettings: { maxTouchPoints: 5 },
    });
    expect(ids(c)).toEqual([
      'deviceMemoryInvalid',
      'hardwareConcurrencyUnusual',
      'touchPointsUnusual',
    ]);
    const fixed = applyAllFixes(c);
    expect(fixed.deviceSettings.deviceMemory).toBe(8);
    expect(fixed.deviceSettings.hardwareConcurrency).toBe(8);
    expect(fixed.advancedFingerprintSettings.maxTouchPoints).toBe(0);
  });

  it('flags real WebRTC and custom locale behind a proxy, but not without one', () => {
    const risky = {
      advancedFingerprintSettings: { webrtc: 'real' as const },
      basicSettings: { timezone: 'custom' as const },
    };
    expect(ids(config(risky))).toEqual(['webrtcRealWithProxy', 'localeNotFollowingProxy']);
    expect(ids(config({ ...risky, windowInfo: { proxyUuids: [] } }))).toEqual([]);
    const fixed = applyAllFixes(config(risky));
    expect(fixed.advancedFingerprintSettings.webrtc).toBe('replace');
    expect(fixed.basicSettings.timezone).toBe('ip');
  });

  it('flags fingerprints left on real as linkable across profiles', () => {
    const c = config({ advancedFingerprintSettings: { canvas: 'real', audioContext: 'real' } });
    expect(checkConsistency(c)).toEqual([
      expect.objectContaining({ id: 'linkableRealFingerprints', params: { count: 2 } }),
    ]);
  });

  it('checks the Firefox kernel against a Firefox UA', () => {
    const ff =
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:150.0) Gecko/20100101 Firefox/150.0';
    const c = config({ windowInfo: { kernel: 'Firefox 158', userAgent: ff } });
    expect(ids(c)).toEqual(['uaVersionMismatch']);
    expect(applyAllFixes(c).windowInfo.userAgent).toBe(
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:158.0) Gecko/20100101 Firefox/158.0'
    );
  });
});
