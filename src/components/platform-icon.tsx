import { Globe } from 'lucide-react';
import { FaLinkedin } from 'react-icons/fa';
import { SiAmazon, SiFacebook, SiGoogle, SiInstagram, SiTiktok, SiX, SiYoutube } from 'react-icons/si';
import type { IconType } from 'react-icons';
import { cn } from '@/lib/utils';

// 平台品牌图标（react-icons 内置 SVG，不依赖 Font Awesome 字体）
const PLATFORM_ICONS: { match: string[]; icon: IconType; className: string }[] = [
  { match: ['facebook.com', 'fb.com'], icon: SiFacebook, className: 'text-[#0866FF]' },
  { match: ['youtube.com'], icon: SiYoutube, className: 'text-[#FF0000]' },
  { match: ['instagram.com'], icon: SiInstagram, className: 'text-[#E4405F]' },
  { match: ['linkedin.com'], icon: FaLinkedin, className: 'text-[#0A66C2]' },
  { match: ['twitter.com', 'x.com'], icon: SiX, className: 'text-foreground' },
  { match: ['tiktok.com'], icon: SiTiktok, className: 'text-foreground' },
  { match: ['google.com', 'gmail.com'], icon: SiGoogle, className: 'text-[#4285F4]' },
  { match: ['amazon.com'], icon: SiAmazon, className: 'text-[#FF9900]' },
];

interface PlatformIconProps {
  url?: string | null;
  className?: string;
}

/** 根据平台 URL 显示品牌图标，未知平台显示地球图标 */
export function PlatformIcon({ url, className }: PlatformIconProps) {
  const lower = (url || '').toLowerCase();
  const hit = PLATFORM_ICONS.find((item) => item.match.some((m) => lower.includes(m)));
  if (hit) {
    const Icon = hit.icon;
    return <Icon className={cn('h-4 w-4 shrink-0', hit.className, className)} />;
  }
  return <Globe className={cn('h-4 w-4 shrink-0 text-foreground/70', className)} />;
}
