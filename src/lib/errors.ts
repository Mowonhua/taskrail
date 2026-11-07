/** 文件职责：把 JavaScript 与平台端口错误转换为可展示的文本。 */

/**
 * 函数职责：保留错误代码和消息，供界面展示及调用方识别平台错误。
 * 输出说明：Error、字符串和含 message 的平台对象保留原有文本，其他值使用字符串表示。
 */
export function errorMessage(error: unknown): string {
  if (error instanceof Error) return `${'code' in error ? String(error.code) + ': ' : ''}${error.message}`;
  if (typeof error === 'string') return error;
  if (error && typeof error === 'object' && 'message' in error) return `${'code' in error ? String(error.code) + ': ' : ''}${String(error.message)}`;
  return String(error);
}
