/**
 * 文件职责：识别手机导航抽屉的触屏手势。
 * 定义范围：手势起点区域、方向判定与结束清理；不接管正文触摸事件。
 */

/** 抽屉手势区占视口的比例：宽取一半、高取四分之三，水平垂直居中，覆盖正文主体而不贴屏幕边缘。 */
const GESTURE_REGION_WIDTH = 0.5;
const GESTURE_REGION_HEIGHT = 0.75;

/**
 * 函数职责：判断触摸起点是否落在视口正中的抽屉手势区内。
 * 输入说明：x、y 为视口坐标。
 * 输出说明：起点在区域内返回 true。
 * 实现思路：区域以视口中心为原点按比例展开，界面尺寸或方向变化时按当次事件重新计算，不缓存旧尺寸。
 */
function inDrawerGestureRegion(x: number, y: number): boolean {
  const width = window.innerWidth, height = window.innerHeight;
  return Math.abs(x - width / 2) <= width * GESTURE_REGION_WIDTH / 2
    && Math.abs(y - height / 2) <= height * GESTURE_REGION_HEIGHT / 2;
}

/**
 * 结构职责：提供抽屉当前状态及唯一开关出口。
 * 字段说明：blocked 表示已有弹窗或界面锁，不允许导航手势。
 * 约束条件：所有函数均在事件发生时读取当前状态。
 */
export interface DrawerGestureOptions {
  isOpen: () => boolean;
  blocked: () => boolean;
  setOpen: (open: boolean) => void;
}

/**
 * 函数职责：为窗口注册手机抽屉左右滑动识别。
 * 输入说明：仅挂载到手机界面；无论开合都只接受视口正中手势区内的起点，打开时还要求起点在侧栏或遮罩上。
 * 输出说明：返回卸载函数；多指、纵向滚动、输入与文本选择不触发抽屉。
 * 实现思路：记录单指起点，在结束时判断水平距离和方向，不取消浏览器默认事件。
 */
export function registerDrawerGestures(options: DrawerGestureOptions): () => void {
  let start: { x: number; y: number; open: boolean; time: number } | undefined;
  const begin = (event: TouchEvent) => {
    start = undefined;
    if (event.touches.length !== 1 || options.blocked()) return;
    const target = event.target;
    if (!(target instanceof Element) || target.closest('input, textarea, select, [role="dialog"], [role="menu"], [data-global-search]')) return;
    if (window.getSelection()?.toString()) return;
    const touch = event.touches[0], open = options.isOpen();
    if (!inDrawerGestureRegion(touch.clientX, touch.clientY) || (open && !target.closest('.sidebar, .drawer-scrim'))) return;
    start = { x: touch.clientX, y: touch.clientY, open, time: event.timeStamp };
  };
  const move = (event: TouchEvent) => {
    if (!start) return;
    // 合并或延迟送达的事件可能晚于下一轮手势，早于本轮起点的事件不能参与方向判断。
    if (event.timeStamp < start.time) return;
    // 一旦确认纵向滚动就放弃本轮，不能在抬手时因回摆又识别为横滑。
    if (event.touches.length !== 1 || Math.abs(event.touches[0].clientY - start.y) > 32) start = undefined;
  };
  const end = (event: TouchEvent) => {
    const origin = start;
    if (!origin || event.timeStamp < origin.time) return;
    start = undefined;
    if (event.changedTouches.length !== 1 || options.blocked() || origin.open !== options.isOpen()) return;
    const touch = event.changedTouches[0], dx = touch.clientX - origin.x, dy = touch.clientY - origin.y;
    if (Math.abs(dx) < 64 || Math.abs(dx) < Math.abs(dy) * 2 || window.getSelection()?.toString()) return;
    if (origin.open ? dx < 0 : dx > 0) options.setOpen(!origin.open);
  };
  const cancel = () => { start = undefined; };
  window.addEventListener('touchstart', begin, { passive: true, capture: true });
  window.addEventListener('touchmove', move, { passive: true, capture: true });
  window.addEventListener('touchend', end, { passive: true, capture: true });
  window.addEventListener('touchcancel', cancel, { passive: true, capture: true });
  return () => {
    window.removeEventListener('touchstart', begin, true);
    window.removeEventListener('touchmove', move, true);
    window.removeEventListener('touchend', end, true);
    window.removeEventListener('touchcancel', cancel, true);
  };
}
