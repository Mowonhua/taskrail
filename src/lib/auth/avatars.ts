/**
 * 文件职责：提供用户账号校验、系统预制头像列表、首字母提取与上传图片编解码工具。
 * 约束条件：严格对齐后端 /api/v1/auth/* 契约（名称 1–64 字符、密码 12–128 字符、头像上传上限 256 KiB）。
 */
import type {
  AuthAvatarMediaType,
  AuthAvatarPreset,
  AuthAvatarPresetId,
  UserAvatar,
} from '../api/types';
import { translate } from '../i18n';

export const AUTH_NAME_MIN_LENGTH = 1;
export const AUTH_NAME_MAX_LENGTH = 64;
export const AUTH_PASSWORD_MIN_LENGTH = 12;
export const AUTH_PASSWORD_MAX_LENGTH = 128;
export const AUTH_AVATAR_UPLOAD_MAX_BYTES = 256 * 1024;
export const AUTH_AVATAR_UPLOAD_MAX_DIMENSION = 1024;

export const AUTH_AVATAR_PRESET_IDS: readonly AuthAvatarPresetId[] = [
  'slate',
  'blue',
  'green',
  'amber',
  'purple',
  'rose',
];

export const DEFAULT_AUTH_AVATAR_PRESETS: readonly AuthAvatarPreset[] = [
  { id: 'slate', label: '石板灰' },
  { id: 'blue', label: '蓝色' },
  { id: 'green', label: '绿色' },
  { id: 'amber', label: '琥珀色' },
  { id: 'purple', label: '紫色' },
  { id: 'rose', label: '玫瑰色' },
];

const PRESET_ID_SET = new Set<string>(AUTH_AVATAR_PRESET_IDS);
const ALLOWED_UPLOAD_MEDIA_TYPES = new Set<AuthAvatarMediaType>([
  'image/png',
  'image/jpeg',
  'image/webp',
]);

const graphemeSegmenter =
  typeof Intl !== 'undefined' && typeof Intl.Segmenter === 'function'
    ? new Intl.Segmenter(undefined, { granularity: 'grapheme' })
    : null;

export function isPresetAvatarId(id: string): id is AuthAvatarPresetId {
  return PRESET_ID_SET.has(id);
}

export function getAvatarInitial(name: string | null | undefined): string {
  const trimmed = (name ?? '').trim();
  if (!trimmed) return '?';
  const first = graphemeSegmenter
    ? graphemeSegmenter.segment(trimmed)[Symbol.iterator]().next().value?.segment
    : Array.from(trimmed)[0];
  if (!first) return '?';
  return first.toLocaleUpperCase();
}

export function validateUserNameError(raw: string): string | null {
  const trimmed = raw.trim();
  const count = Array.from(trimmed).length;
  if (count < AUTH_NAME_MIN_LENGTH || count > AUTH_NAME_MAX_LENGTH) {
    return translate('名称须为 1 至 64 个字符。');
  }
  if (/[\u0000-\u001f\u007f]/.test(trimmed)) {
    return translate('名称不能包含控制字符。');
  }
  return null;
}

export function validateUserPasswordError(password: string): string | null {
  const count = Array.from(password).length;
  if (count < AUTH_PASSWORD_MIN_LENGTH || count > AUTH_PASSWORD_MAX_LENGTH) {
    return translate('密码须为 12 至 128 个字符。');
  }
  return null;
}

export function decodeBase64ByteLength(dataBase64: string): number | null {
  const normalized = dataBase64.replace(/\s+/g, '');
  if (!normalized || normalized.length % 4 !== 0) {
    return null;
  }
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(normalized)) {
    return null;
  }
  try {
    const binary = atob(normalized);
    return binary.length;
  } catch {
    return null;
  }
}

export function validateUserAvatar(avatar: UserAvatar): string | null {
  if (avatar.kind === 'preset') {
    if (!isPresetAvatarId(avatar.id)) {
      return translate('头像预设不存在。');
    }
    return null;
  }
  if (avatar.kind === 'upload') {
    if (!ALLOWED_UPLOAD_MEDIA_TYPES.has(avatar.media_type)) {
      return translate('头像仅支持 PNG、JPEG 或 WebP 图片。');
    }
    const byteLength = decodeBase64ByteLength(avatar.data_base64);
    if (byteLength === null || byteLength === 0) {
      return translate('头像图片数据无效。');
    }
    if (byteLength > AUTH_AVATAR_UPLOAD_MAX_BYTES) {
      return translate('头像图片不能超过 256 KiB。');
    }
    return null;
  }
  return translate('头像格式无效。');
}

export function avatarToDataUrl(avatar: UserAvatar | null | undefined): string | null {
  if (!avatar || avatar.kind !== 'upload') {
    return null;
  }
  if (!ALLOWED_UPLOAD_MEDIA_TYPES.has(avatar.media_type) || !avatar.data_base64) {
    return null;
  }
  return `data:${avatar.media_type};base64,${avatar.data_base64}`;
}

export function avatarsEqual(
  left: UserAvatar | null | undefined,
  right: UserAvatar | null | undefined,
): boolean {
  const a = left ?? null;
  const b = right ?? null;
  if (a === b) return true;
  if (!a || !b || a.kind !== b.kind) return false;
  if (a.kind === 'preset' && b.kind === 'preset') {
    return a.id === b.id;
  }
  if (a.kind === 'upload' && b.kind === 'upload') {
    return a.media_type === b.media_type && a.data_base64 === b.data_base64;
  }
  return false;
}

function uint8ArrayToBase64(bytes: Uint8Array): string {
  const chunkSize = 0x8000;
  const parts: string[] = [];
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    const chunk = bytes.subarray(offset, offset + chunkSize);
    parts.push(String.fromCharCode(...chunk));
  }
  return btoa(parts.join(''));
}

function canvasToBlob(
  canvas: HTMLCanvasElement,
  mediaType: 'image/webp' | 'image/jpeg',
  quality: number,
): Promise<Blob | null> {
  return new Promise((resolve) => {
    canvas.toBlob((blob) => resolve(blob), mediaType, quality);
  });
}

/**
 * 将超出体积或尺寸限制的位图居中裁切为正方形，并逐步降低分辨率与质量压缩至 256 KiB 以内。
 * 注意：仅在需要压缩或缩放时触发，符合限制的原图保留原始字节与格式。
 */
async function compressAvatarBitmap(
  bitmap: ImageBitmap,
): Promise<Extract<UserAvatar, { kind: 'upload' }> | null> {
  if (typeof document === 'undefined') return null;
  const canvas = document.createElement('canvas');
  if (typeof canvas.getContext !== 'function' || typeof canvas.toBlob !== 'function') {
    return null;
  }
  const ctx = canvas.getContext('2d');
  if (!ctx) return null;

  const side = Math.min(bitmap.width, bitmap.height);
  const sx = Math.floor((bitmap.width - side) / 2);
  const sy = Math.floor((bitmap.height - side) / 2);
  const maxDim = Math.min(side, AUTH_AVATAR_UPLOAD_MAX_DIMENSION);
  const targetSizes = Array.from(
    new Set([maxDim, 512, 384, 256, 192].filter((size) => size > 0 && size <= maxDim)),
  );
  const qualities = [0.88, 0.76, 0.64, 0.52, 0.4];
  const formats: readonly ('image/webp' | 'image/jpeg')[] = ['image/webp', 'image/jpeg'];

  for (const targetSize of targetSizes) {
    canvas.width = targetSize;
    canvas.height = targetSize;
    ctx.clearRect(0, 0, targetSize, targetSize);
    ctx.drawImage(bitmap, sx, sy, side, side, 0, 0, targetSize, targetSize);

    for (const format of formats) {
      for (const quality of qualities) {
        const blob = await canvasToBlob(canvas, format, quality);
        if (!blob || blob.type !== format) break;
        if (blob.size > 0 && blob.size <= AUTH_AVATAR_UPLOAD_MAX_BYTES) {
          const compressedBytes = new Uint8Array(await blob.arrayBuffer());
          if (compressedBytes.byteLength <= AUTH_AVATAR_UPLOAD_MAX_BYTES) {
            return {
              kind: 'upload',
              media_type: format,
              data_base64: uint8ArrayToBase64(compressedBytes),
            };
          }
        }
      }
    }
  }
  return null;
}

export async function fileToUploadAvatar(
  file: File,
): Promise<Extract<UserAvatar, { kind: 'upload' }>> {
  const mediaType = file.type as AuthAvatarMediaType;
  if (!ALLOWED_UPLOAD_MEDIA_TYPES.has(mediaType)) {
    throw new Error(translate('头像仅支持 PNG、JPEG 或 WebP 图片。'));
  }
  const buffer = await file.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  if (bytes.byteLength === 0) {
    throw new Error(translate('头像图片数据无效。'));
  }

  let tooLargeBytes = bytes.byteLength > AUTH_AVATAR_UPLOAD_MAX_BYTES;
  let tooLargeDimension = false;

  if (typeof createImageBitmap === 'function') {
    let bitmap: ImageBitmap | null = null;
    try {
      bitmap = await createImageBitmap(file);
      tooLargeDimension =
        bitmap.width > AUTH_AVATAR_UPLOAD_MAX_DIMENSION ||
        bitmap.height > AUTH_AVATAR_UPLOAD_MAX_DIMENSION;
      if (tooLargeBytes || tooLargeDimension) {
        const compressed = await compressAvatarBitmap(bitmap);
        if (compressed) {
          return compressed;
        }
      }
    } catch {
      // 测试环境或部分浏览器无法在本地解码时，若体积未超限则交由服务端校验。
    } finally {
      bitmap?.close?.();
    }
  }

  if (tooLargeBytes) {
    throw new Error(translate('头像图片不能超过 256 KiB。'));
  }
  if (tooLargeDimension) {
    throw new Error(translate('头像图片宽高不能超过 1024 像素。'));
  }
  return {
    kind: 'upload',
    media_type: mediaType,
    data_base64: uint8ArrayToBase64(bytes),
  };
}
