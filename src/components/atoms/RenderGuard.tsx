import { Component, type ReactNode } from "react";

interface Props {
  /** O que aparece no lugar do que falhou ao desenhar; `reset` tenta de novo. */
  fallback: (error: Error, reset: () => void) => ReactNode;
  /** Mudou (outra tela, outro texto): tenta desenhar de novo. */
  resetKey?: unknown;
  children: ReactNode;
}

interface State { error: Error | null; key: unknown }

/** Segura o erro de quem desenha dentro dele. Sem isto, uma exceção no meio do
 * desenho (uma resposta do modelo que o leitor de Markdown não esperava, por
 * exemplo) desmontava o app inteiro e deixava a janela em branco. */
export class RenderGuard extends Component<Props, State> {
  state: State = { error: null, key: this.props.resetKey };

  static getDerivedStateFromError(error: unknown): Partial<State> {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  static getDerivedStateFromProps(props: Props, state: State): Partial<State> | null {
    return Object.is(props.resetKey, state.key) ? null : { error: null, key: props.resetKey };
  }

  componentDidCatch(error: unknown) {
    console.error("render", error);
  }

  reset = () => this.setState({ error: null });

  render() {
    return this.state.error ? this.props.fallback(this.state.error, this.reset) : this.props.children;
  }
}
