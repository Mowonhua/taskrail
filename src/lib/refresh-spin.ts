/**
 * 文件职责：控制刷新按钮图标的最短旋转展示时长；不发起刷新请求。
 * 定义范围：旋转计时器与最小时长；组件负责把旋转状态映射到按钮。
 */

/** 与 app.css 中 refresh-spin 的 0.8s 周期一致，保证快速请求也完整播放至少一轮。 */
export const REFRESH_SPIN_MIN_MS = 800;

/**
 * 函数职责：创建一个只管理图标旋转状态的最短计时器。
 * 输入说明：onChange 由组件传入，用于同步 Svelte 旋转状态。
 * 输出说明：begin 立即开始旋转并作废旧计时；settle 在请求结束后补足最短时长；stop 清理计时并停止旋转。
 */
export function createRefreshSpin(onChange: (spinning: boolean) => void): {
  begin: () => number;
  settle: (session: number) => void;
  stop: () => void;
} {
  let session = 0;
  let startedAt = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  return {
    begin() {
      session += 1;
      startedAt = performance.now();
      if (timer !== undefined) {
        clearTimeout(timer);
        timer = undefined;
      }
      onChange(true);
      return session;
    },
    settle(current) {
      if (current !== session) return;
      if (timer !== undefined) clearTimeout(timer);
      const remaining = REFRESH_SPIN_MIN_MS - (performance.now() - startedAt);
      timer = setTimeout(
        () => {
          timer = undefined;
          if (current === session) onChange(false);
        },
        Math.max(0, remaining),
      );
    },
    stop() {
      session += 1;
      if (timer !== undefined) {
        clearTimeout(timer);
        timer = undefined;
      }
      onChange(false);
    },
  };
}
