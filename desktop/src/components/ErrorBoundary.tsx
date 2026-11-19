import { Component, type ErrorInfo, type ReactNode } from "react";
import { Icon } from "./ui/icon";
import { Button } from "./ui/button";

/**
 * 全局错误边界（P0-4）：捕获子组件渲染异常，显示友好占位页而非白屏。
 * WHY：此前各组件 invoke 失败是散落的 catch/toast，没有统一兜底。
 * ErrorBoundary 捕获 React 渲染生命周期中的同步错误，配合 ConnectionAlert
 * （状态拉取失败提示条）形成完整的异常兜底体系。
 */
interface Props {
  children: ReactNode;
  /** 出错时替换默认占位页。传函数可拿到真实 error（独立窗口排查全靠它）。 */
  fallback?: ReactNode | ((error: Error | null) => ReactNode);
  /** 用于错误文案：告诉用户「哪一块」崩了，而不只是「应用遇到错误」。 */
  componentName?: string;
}

interface State {
  hasError: boolean;
  error?: Error;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // 独立窗口（工作台）没有 DevTools 可开，崩溃信息必须同时留在 console 与界面上，
    // 否则表现就是「窗口白屏，日志里什么都没有」。
    console.error(`[ErrorBoundary${this.props.componentName ? `/${this.props.componentName}` : ""}]`, error, info.componentStack);
  }

  handleRetry = () => {
    this.setState({ hasError: false, error: undefined });
  };

  // H9 修复：「重新加载」只重置 React 状态，若崩溃根源是持久化查询缓存/外部状态，很可能立即复现；
  // 再提供一个真正刷新整个页面的最后手段。
  handleFullReload = () => {
    window.location.reload();
  };

  render() {
    if (this.state.hasError) {
      const { fallback, componentName } = this.props;
      if (typeof fallback === "function") return fallback(this.state.error ?? null);
      if (fallback !== undefined) return fallback;
      const what = componentName ? `「${componentName}」加载失败` : "应用遇到错误";
      return (
        <div className="eb-mask flex h-screen flex-col items-center justify-center gap-4 bg-background p-8 text-center">
          <div className="eb flex flex-col items-center gap-4 text-center">
            <div className="flex h-16 w-16 items-center justify-center rounded-full bg-destructive/10 eb-ico">
              <Icon name="alertTriangle" size={32} className="text-destructive-ink" />
            </div>
            <div className="space-y-1.5">
              <h3 className="text-lg font-semibold text-foreground">{what}</h3>
              <p className="max-w-md text-sm text-muted-foreground">
                页面渲染过程中发生了异常。可以尝试重新加载，如果问题持续请重启应用。
              </p>
            </div>
            {this.state.error?.message && (
              <pre className="max-w-lg overflow-auto rounded-md bg-muted p-3 text-left text-xs text-muted-foreground eb-stack">
                {this.state.error.message}
              </pre>
            )}
            <div className="flex items-center gap-2 eb-act">
              <Button onClick={this.handleRetry}>
                <Icon name="refresh" size={16} />
                重新加载
              </Button>
              <Button variant="outline" onClick={this.handleFullReload}>
                <Icon name="refresh" size={16} />
                完全刷新
              </Button>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
