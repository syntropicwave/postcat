import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}
interface State {
  error: Error | null;
}

/**
 * Last-resort guard: a render error anywhere below unmounts the whole React
 * tree (leaving only the native window controls). Catch it and show a
 * recoverable message instead of a blank window.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Unhandled render error:", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="crash-screen">
          <h2>Something broke.</h2>
          <pre>{this.state.error.message}</pre>
          <div className="crash-actions">
            <button
              className="primary"
              onClick={() => this.setState({ error: null })}
            >
              Try again
            </button>
            <button onClick={() => window.location.reload()}>Reload</button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
