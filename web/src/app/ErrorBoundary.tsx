import { Component, type ErrorInfo, type ReactNode } from "react";
import { interaction } from "../state/session";

export class ErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    interaction.set({ ...interaction.get(), panel: "settings" });
    console.error("React overlay failed", error, info);
  }

  render() {
    if (this.state.failed) {
      return (
        <section className="loading" role="alert" data-keyboard-owner="true">
          <div>
            <h1>The interface could not render.</h1>
            <p>Your saved profiles have not been deleted. See the developer console.</p>
            <button onClick={() => location.reload()}>Reload interface</button>
          </div>
        </section>
      );
    }
    return this.props.children;
  }
}
