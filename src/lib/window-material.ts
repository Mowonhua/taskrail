/**
 * 文件职责：协调原生窗口材质与网页透明状态。
 * 定义范围：串行应用平台材质、忽略过期结果、失败时恢复实色。
 */

/** 原生适配器只接收固定材质名称；opaque 为缺省及双色主题的回退。 */
export type WindowMaterial = 'opaque' | 'transparent' | 'blur' | 'acrylic';

/**
 * 结构职责：保持原生窗口变更与透明 CSS 开关一致。
 * 约束条件：只有最新请求成功才可透明；原生调用必须串行，避免旧效果晚于新效果落地。
 */
export class WindowMaterialController {
  private queue: Promise<void> = Promise.resolve();
  private revision = 0;
  private veilTimer: ReturnType<typeof setTimeout> | undefined;
  private veilCleanup: (() => void) | undefined;

  /** applyNative 返回 false 表示平台不支持，抛错表示应用失败；两者都保持实色。 */
  constructor(private root: HTMLElement, private applyNative: (material: WindowMaterial, theme: 'light' | 'dark' | 'system', keepTransparentOnBlur: boolean) => Promise<boolean>) {}

  /**
   * 函数职责：按请求顺序切换原生材质，只发布最后一次请求的网页透明状态。
   * 输入说明：material 已由主题校验限制；旧主题传 opaque；theme 保留 system 以解除原生显式主题；否则传当前浅深模式。keepTransparentOnBlur 缺省 false，独立于主题包，仅由支持的原生材质消费。
   * 输出说明：完成后原生状态已应用，错误传给调用方且不阻断后续切换。
   * 实现思路：切到实色或尚未透明时先同步关闭网页透明；已透明状态下调整明暗或失焦偏好期间保持透明避免闪回实色，若最终不支持或失败再回落实色。
   */
  update(material: WindowMaterial, theme: 'light' | 'dark' | 'system' = 'light', keepTransparentOnBlur = false): Promise<void> {
    const revision = ++this.revision;
    if (material === 'opaque' || this.root.dataset.windowTransparent !== 'true') {
      this.root.dataset.windowTransparent = 'false';
      this.clearBackdropVeil();
    }
    const operation = this.queue.then(async () => {
      const supported = await this.applyNative(material, theme, keepTransparentOnBlur);
      if (revision === this.revision) {
        const transparent = supported && material !== 'opaque';
        this.root.dataset.windowTransparent = String(transparent);
        if (!transparent) this.clearBackdropVeil();
      }
    }).catch(error => {
      if (revision === this.revision) {
        this.root.dataset.windowTransparent = 'false';
        this.clearBackdropVeil();
      }
      throw error;
    });
    // 吞掉的仅是内部队列错误，当前调用方仍收到原始异常；后续主题或偏好切换可继续恢复。
    this.queue = operation.catch(() => {});
    return operation;
  }

  /**
   * 函数职责：在透明窗口显示或从最小化恢复期间启用短暂的雾面过渡遮罩，避免系统缩放动画期背景清晰透出后突然变糊。
   * 输入说明：phase 为 'hold' 时先锁定遮罩以等待首帧显示；'fade' 时执行保持并淡出动画。
   * 输出说明：仅在透明材质激活且未开启减少动态效果时生效；动画结束或兜底超时后自动清理属性。
   */
  showBackdropVeil(phase: 'hold' | 'fade' = 'fade'): void {
    if (this.root.dataset.windowTransparent !== 'true' || window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) {
      this.clearBackdropVeil();
      return;
    }
    if (phase === 'hold') {
      this.clearBackdropVeil();
      this.root.dataset.backdropVeil = 'active';
      return;
    }
    const next = this.root.dataset.backdropVeil === 'fade-a' ? 'fade-b' : 'fade-a';
    this.clearBackdropVeil();
    this.root.dataset.backdropVeil = next;
    const expectedName = `backdrop-veil-${next}`;
    const onAnimationEnd = (event: AnimationEvent) => {
      const animation = (event as AnimationEvent & { animation?: Animation }).animation;
      const pseudo = (animation?.effect as KeyframeEffect | null | undefined)?.pseudoElement;
      if (pseudo && pseudo !== '::before') return;
      if (!pseudo && event.animationName !== expectedName) return;
      if (this.root.dataset.backdropVeil === next) this.clearBackdropVeil();
    };
    this.root.addEventListener('animationend', onAnimationEnd);
    this.veilCleanup = () => this.root.removeEventListener('animationend', onAnimationEnd);
    // 动画未运行或被系统打断时的兜底回收，避免遮罩长期残留。
    this.veilTimer = setTimeout(() => {
      if (this.root.dataset.backdropVeil === next) this.clearBackdropVeil();
    }, 450);
  }

  clearBackdropVeil(): void {
    if (this.veilTimer !== undefined) {
      clearTimeout(this.veilTimer);
      this.veilTimer = undefined;
    }
    this.veilCleanup?.();
    this.veilCleanup = undefined;
    delete this.root.dataset.backdropVeil;
  }

  dispose(): void {
    this.clearBackdropVeil();
  }
}
