/** 文件职责：提供设置下拉菜单共用的锚点定位与展开期间的事件生命周期。 */

/**
 * 函数职责：把已展开的浮层限制在可视视口内；锚点完全离开裁切祖先时请求关闭。
 * 输入说明：调用方须先更新 DOM 并显示 popover，浮层采用 fixed 定位。
 * 输出说明：只更新浮层几何；不改变焦点、选择值或组件的展开状态。
 */
export function positionAnchoredPopover(trigger: HTMLElement, popup: HTMLElement, close: () => void): void {
  const bounds = trigger.getBoundingClientRect();
  // 顶层浮层不受父容器裁切；锚点滚出任一裁切祖先时收起，避免菜单脱离字段悬浮。
  for (let ancestor = trigger.parentElement; ancestor; ancestor = ancestor.parentElement) {
    const style = getComputedStyle(ancestor);
    const clip = ancestor.getBoundingClientRect();
    if ((/auto|scroll|hidden|clip/.test(style.overflowY) && (bounds.bottom <= clip.top || bounds.top >= clip.bottom)) ||
        (/auto|scroll|hidden|clip/.test(style.overflowX) && (bounds.right <= clip.left || bounds.left >= clip.right))) {
      close();
      return;
    }
  }
  const margin = 8;
  const gap = 4;
  const viewport = window.visualViewport;
  const viewportTop = viewport?.offsetTop ?? 0;
  const viewportLeft = viewport?.offsetLeft ?? 0;
  const viewportHeight = viewport?.height ?? window.innerHeight;
  const viewportWidth = viewport?.width ?? window.innerWidth;
  const below = viewportTop + viewportHeight - bounds.bottom - gap - margin;
  const above = bounds.top - viewportTop - gap - margin;
  const desiredHeight = Math.min(popup.scrollHeight, 320);
  const placeAbove = below < desiredHeight && above > below;
  const availableHeight = Math.max(0, placeAbove ? above : below);
  const width = Math.min(bounds.width, Math.max(0, viewportWidth - margin * 2));
  popup.style.width = `${width}px`;
  popup.style.maxHeight = `${Math.min(320, availableHeight)}px`;
  popup.style.left = `${Math.max(viewportLeft + margin, Math.min(bounds.left, viewportLeft + viewportWidth - margin - width))}px`;
  popup.style.top = `${placeAbove ? Math.max(viewportTop + margin, bounds.top - gap - popup.getBoundingClientRect().height) : bounds.bottom + gap}px`;
}

/**
 * 函数职责：在展开期间处理外部按下、滚动和视口变化，返回卸载清理函数。
 * 输入说明：每次展开建立一次监听，收起或卸载时必须调用返回值。
 * 输出说明：外部按下请求关闭，滚动和视口变化重新定位；清理还会退出 manual popover 顶层。
 * 实现思路：不监听 focusout，避免滚动条取得焦点时误关菜单。
 */
export function observeAnchoredPopover(trigger: HTMLElement, popup: HTMLElement, close: () => void): () => void {
  const position = () => positionAnchoredPopover(trigger, popup, close);
  const onPointerdown = (event: PointerEvent) => {
    const path = event.composedPath();
    if (!path.includes(trigger) && !path.includes(popup)) close();
  };
  const onScroll = (event: Event) => { if (event.target !== popup) position(); };
  const viewport = window.visualViewport;
  document.addEventListener('pointerdown', onPointerdown, true);
  document.addEventListener('scroll', onScroll, true);
  window.addEventListener('resize', position);
  viewport?.addEventListener('resize', position);
  viewport?.addEventListener('scroll', position);
  return () => {
    document.removeEventListener('pointerdown', onPointerdown, true);
    document.removeEventListener('scroll', onScroll, true);
    window.removeEventListener('resize', position);
    viewport?.removeEventListener('resize', position);
    viewport?.removeEventListener('scroll', position);
    if (typeof popup.hidePopover === 'function' && popup.matches(':popover-open')) popup.hidePopover();
  };
}
