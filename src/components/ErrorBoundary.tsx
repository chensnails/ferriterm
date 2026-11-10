import { Component, type ErrorInfo, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

interface Labels {
  title: string;
  detail: string;
  retry: string;
  reload: string;
}

class Boundary extends Component<{ children: ReactNode; labels: Labels }, { error: Error | null }> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Ferriterm render error:", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="ui-error-boundary" role="alert">
        <h2>{this.props.labels.title}</h2>
        <p className="hint">{this.props.labels.detail}</p>
        <pre className="ui-error-detail">{this.state.error.message}</pre>
        <div className="ui-error-actions">
          <button type="button" onClick={() => this.setState({ error: null })}>
            {this.props.labels.retry}
          </button>
          <button type="button" className="primary" onClick={() => window.location.reload()}>
            {this.props.labels.reload}
          </button>
        </div>
      </div>
    );
  }
}

export function ErrorBoundary({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  return (
    <Boundary
      labels={{
        title: t("errors.boundaryTitle"),
        detail: t("errors.boundaryDetail"),
        retry: t("errors.retryUi"),
        reload: t("errors.reloadApp"),
      }}
    >
      {children}
    </Boundary>
  );
}
