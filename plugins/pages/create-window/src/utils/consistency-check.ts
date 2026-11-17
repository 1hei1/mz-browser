/**
 * 环境指纹一致性自检
 *
 * 检测站（CreepJS、pixelscan 等）主要看各项指纹之间是否互相矛盾：
 * Windows 的 UA 配 Apple 显卡、UA 版本号跟内核实际能力对不上、美国代理配自定义时区……
 * 这里只做纯函数检查，返回问题列表和可选的一键修复。
 */

import type { WindowConfig } from '../types';
import { generateUserAgentByKernel } from './user-agent-generator';

export type ConsistencyLevel = 'error' | 'warning';

export interface ConsistencyIssue {
  /** 对应 i18n 键 consistency.issues.<id> */
  id: string;
  level: ConsistencyLevel;
  /** 文案插值参数 */
  params?: Record<string, string | number>;
  /** 一键修复；没有表示需要用户自己调整 */
  fix?: (config: WindowConfig) => WindowConfig;
}

/** Chrome 报出的 navigator.deviceMemory 只会是这些值（上限 8） */
const VALID_DEVICE_MEMORY = [0.25, 0.5, 1, 2, 4, 8];

/** 内核资源名（"Chrome 158" / "Firefox 158"）里的主版本号 */
export function kernelMajorVersion(kernel: string | undefined): number | undefined {
  const match = kernel?.match(/(\d+)/);
  return match ? Number(match[1]) : undefined;
}

export function isFirefoxKernel(kernel: string | undefined): boolean {
  return kernel?.startsWith('Firefox') ?? false;
}

function hasProxy(config: WindowConfig): boolean {
  const info = config.windowInfo;
  return info.proxySourceMode === 'local'
    ? info.localProxyNodeNames.length > 0
    : info.proxyUuids.length > 0;
}

/** UA 里声明的系统 */
function uaSystem(ua: string): 'Windows' | 'macOS' | 'Linux' | 'Android' | 'iOS' | undefined {
  if (/Windows NT/.test(ua)) return 'Windows';
  if (/iPhone|iPad/.test(ua)) return 'iOS';
  if (/Android/.test(ua)) return 'Android';
  if (/Macintosh|Mac OS X/.test(ua)) return 'macOS';
  if (/Linux|X11|CrOS/.test(ua)) return 'Linux';
  return undefined;
}

/** WebGL 渲染器字符串对应的系统（ANGLE 后端能看出来） */
function rendererSystem(renderer: string): 'Windows' | 'macOS' | 'Linux' | 'Mobile' | undefined {
  if (/Direct3D|D3D11|D3D9/i.test(renderer)) return 'Windows';
  if (/Metal|Apple M\d|Apple GPU/i.test(renderer)) return 'macOS';
  if (/Mali|Adreno|PowerVR/i.test(renderer)) return 'Mobile';
  if (/OpenGL|Vulkan|Mesa/i.test(renderer)) return 'Linux';
  return undefined;
}

function nearestDeviceMemory(value: number): number {
  return VALID_DEVICE_MEMORY.reduce((best, candidate) =>
    Math.abs(candidate - value) < Math.abs(best - value) ? candidate : best
  );
}

function regenerateUserAgent(config: WindowConfig): WindowConfig {
  const { system, kernel } = config.windowInfo;
  return {
    ...config,
    windowInfo: {
      ...config.windowInfo,
      userAgent: generateUserAgentByKernel(
        system,
        isFirefoxKernel(kernel) ? 'Firefox' : 'Chrome',
        kernel.match(/\d+/)?.[0]
      ),
    },
  };
}

export function checkConsistency(config: WindowConfig): ConsistencyIssue[] {
  const issues: ConsistencyIssue[] = [];
  const { windowInfo, basicSettings, advancedFingerprintSettings: fp, deviceSettings } = config;
  const ua = windowInfo.userAgent || '';
  const firefox = isFirefoxKernel(windowInfo.kernel);
  const kernelMajor = kernelMajorVersion(windowInfo.kernel);
  const proxied = hasProxy(config);

  // 1. UA 的浏览器类型要和内核一致：Chromium 内核报 Safari/Firefox，JS 特性一测就露馅
  if (ua) {
    const claimsChromium = /Chrome\/\d+/.test(ua) && !/Firefox\//.test(ua);
    const claimsFirefox = /Firefox\/\d+/.test(ua);
    if ((!firefox && !claimsChromium) || (firefox && !claimsFirefox)) {
      issues.push({
        id: 'uaBrowserMismatch',
        level: 'error',
        params: { kernel: windowInfo.kernel },
        fix: regenerateUserAgent,
      });
    } else if (kernelMajor) {
      // 2. 主版本号要等于内核主版本（Chromium 内核启动时也会强制改过来，这里提前提示）
      const declared = Number(ua.match(firefox ? /Firefox\/(\d+)/ : /Chrome\/(\d+)/)?.[1]);
      if (declared && declared !== kernelMajor) {
        issues.push({
          id: 'uaVersionMismatch',
          level: 'warning',
          params: { declared, kernel: kernelMajor },
          // 只改版本号，保留 Edge 等品牌
          fix: (c) => ({
            ...c,
            windowInfo: {
              ...c.windowInfo,
              userAgent: c.windowInfo.userAgent.replace(
                firefox ? /\b(Firefox|rv:)(\/|)(\d+)(\.\d+)*/g : /\b(Chrome|Edg)(\/)(\d+)(\.\d+)*/g,
                (_m, name: string, sep: string) =>
                  name === 'rv:' ? `rv:${kernelMajor}.0` : firefox ? `${name}${sep}${kernelMajor}.0` : `${name}${sep}${kernelMajor}.0.0.0`
              ),
            },
          }),
        });
      }
    }

    // 3. UA 声明的系统要和“系统”选项一致
    const declaredSystem = uaSystem(ua);
    if (declaredSystem && windowInfo.system && declaredSystem !== windowInfo.system) {
      issues.push({
        id: 'uaSystemMismatch',
        level: 'error',
        params: { ua: declaredSystem, system: windowInfo.system },
        fix: regenerateUserAgent,
      });
    }
  }

  // 4. 内核是 Windows 版：伪装成 macOS/Linux 时字体渲染、显卡后端仍是 Windows 的
  if (!firefox && windowInfo.system && windowInfo.system !== 'Windows') {
    issues.push({ id: 'nonWindowsSystem', level: 'warning', params: { system: windowInfo.system } });
  }

  // 5. WebGL 渲染器的后端要和系统一致
  if (fp.webglInfo !== 'real' && fp.webglRenderer) {
    const gpuSystem = rendererSystem(fp.webglRenderer);
    if (gpuSystem && windowInfo.system && gpuSystem !== windowInfo.system) {
      issues.push({
        id: 'webglSystemMismatch',
        level: 'error',
        params: { renderer: fp.webglRenderer, system: windowInfo.system },
      });
    }
  }

  // 6. deviceMemory 只能是 0.25/0.5/1/2/4/8
  if (!VALID_DEVICE_MEMORY.includes(deviceSettings.deviceMemory)) {
    const fixed = nearestDeviceMemory(deviceSettings.deviceMemory);
    issues.push({
      id: 'deviceMemoryInvalid',
      level: 'error',
      params: { value: deviceSettings.deviceMemory, fixed },
      fix: (c) => ({ ...c, deviceSettings: { ...c.deviceSettings, deviceMemory: fixed } }),
    });
  }

  // 7. CPU 核数：真实机器几乎都是偶数，且很少超过 64
  const cores = deviceSettings.hardwareConcurrency;
  if (cores > 64 || (cores > 2 && cores % 2 === 1)) {
    const fixed = Math.min(64, cores + (cores % 2));
    issues.push({
      id: 'hardwareConcurrencyUnusual',
      level: 'warning',
      params: { value: cores, fixed },
      fix: (c) => ({ ...c, deviceSettings: { ...c.deviceSettings, hardwareConcurrency: fixed } }),
    });
  }

  // 8. 桌面系统的触摸点只会是 0（无触屏）或 10（触屏本）
  if (
    (windowInfo.system === 'Windows' || windowInfo.system === 'macOS' || windowInfo.system === 'Linux') &&
    fp.maxTouchPoints !== 0 &&
    fp.maxTouchPoints !== 10
  ) {
    issues.push({
      id: 'touchPointsUnusual',
      level: 'warning',
      params: { value: fp.maxTouchPoints },
      fix: (c) => ({
        ...c,
        advancedFingerprintSettings: { ...c.advancedFingerprintSettings, maxTouchPoints: 0 },
      }),
    });
  }

  // 9. 用了代理却把 WebRTC 设成真实：会把本机 IP 暴露给网站
  if (proxied && fp.webrtc === 'real') {
    issues.push({
      id: 'webrtcRealWithProxy',
      level: 'error',
      fix: (c) => ({
        ...c,
        advancedFingerprintSettings: { ...c.advancedFingerprintSettings, webrtc: 'replace' },
      }),
    });
  }

  // 10. 用了代理时，语言/时区/地理位置不跟随 IP，就可能和代理所在地对不上
  if (proxied) {
    const custom = (['language', 'timezone', 'geolocation'] as const).filter(
      (key) => basicSettings[key] === 'custom'
    );
    if (custom.length > 0) {
      issues.push({
        id: 'localeNotFollowingProxy',
        level: 'warning',
        params: { count: custom.length },
        fix: (c) => ({
          ...c,
          basicSettings: {
            ...c.basicSettings,
            ...Object.fromEntries(custom.map((key) => [key, 'ip'])),
          },
        }),
      });
    }
  }

  // 11. 这些选“真实”时所有环境的值都一样，多个环境之间可以被关联起来
  const real = (['canvas', 'audioContext', 'webglImage', 'clientRects'] as const).filter(
    (key) => fp[key] === 'real'
  );
  if (real.length > 0) {
    issues.push({
      id: 'linkableRealFingerprints',
      level: 'warning',
      params: { count: real.length },
      fix: (c) => ({
        ...c,
        advancedFingerprintSettings: {
          ...c.advancedFingerprintSettings,
          ...Object.fromEntries(real.map((key) => [key, 'random'])),
        },
      }),
    });
  }

  return issues;
}

/** 依次应用所有可自动修复的问题 */
export function applyAllFixes(config: WindowConfig): WindowConfig {
  let next = config;
  // 修一项可能影响另一项（例如重新生成 UA），最多迭代几轮
  for (let round = 0; round < 3; round++) {
    const fixable = checkConsistency(next).filter((issue) => issue.fix);
    if (fixable.length === 0) break;
    for (const issue of fixable) {
      next = issue.fix!(next);
    }
  }
  return next;
}
