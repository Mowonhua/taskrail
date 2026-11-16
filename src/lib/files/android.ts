/**
 * 文件职责：适配 Android 环境的配置持久化与文本导出。
 * 定义范围：复用 TauriFilePort 的配置与外链通道，提供基于系统文档选择器的导出实现。
 */
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { TauriFilePort } from './tauri';

export class AndroidFilePort extends TauriFilePort {
  isMobile = true;

  override async exportText(text: string, filename: string): Promise<boolean> {
    const target = await save({ defaultPath: filename });
    if (!target || Array.isArray(target)) return false;
    await writeTextFile(target, text);
    return true;
  }
}
