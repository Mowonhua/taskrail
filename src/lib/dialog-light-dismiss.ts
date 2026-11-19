/** 在缺少 closedby 的 WebView 中，模拟原生弹窗的轻触关闭。 */
export function lightDismiss(dialog: HTMLDialogElement): { destroy(): void } | void {
  if ('closedBy' in HTMLDialogElement.prototype) return;

  let outsidePointer: number | undefined;
  function outside(event: PointerEvent): boolean {
    if (!dialog.open || event.target !== dialog) return false;
    const rect = dialog.getBoundingClientRect();
    return event.clientX < rect.left || event.clientX > rect.right ||
      event.clientY < rect.top || event.clientY > rect.bottom;
  }
  function pointerDown(event: PointerEvent): void {
    // 按下和抬起都在遮罩上才关闭，避免从正文拖动选区时误关。
    outsidePointer = event.button === 0 && outside(event) ? event.pointerId : undefined;
  }
  function pointerUp(event: PointerEvent): void {
    const shouldClose = outsidePointer !== undefined && outsidePointer === event.pointerId && outside(event);
    outsidePointer = undefined;
    if (!shouldClose) return;
    if (typeof dialog.requestClose === 'function') {
      dialog.requestClose();
    } else if (dialog.dispatchEvent(new Event('cancel', { cancelable: true }))) {
      if (typeof dialog.close === 'function') dialog.close();
      else {
        dialog.removeAttribute('open');
        dialog.dispatchEvent(new Event('close'));
      }
    }
  }
  function clearPointer(): void {
    outsidePointer = undefined;
  }

  dialog.addEventListener('pointerdown', pointerDown);
  dialog.addEventListener('pointerup', pointerUp);
  dialog.addEventListener('pointercancel', clearPointer);
  dialog.addEventListener('close', clearPointer);
  return {
    destroy() {
      dialog.removeEventListener('pointerdown', pointerDown);
      dialog.removeEventListener('pointerup', pointerUp);
      dialog.removeEventListener('pointercancel', clearPointer);
      dialog.removeEventListener('close', clearPointer);
    },
  };
}
