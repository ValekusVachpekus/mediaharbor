import React from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import './style.css';
class ErrorBoundary extends React.Component<{ children: React.ReactNode }, { error?: string }> {
  state: { error?: string } = {};
  static getDerivedStateFromError(error: Error) { return { error: error.message }; }
  render() { return this.state.error ? <main><h1>Не удалось открыть интерфейс</h1><p>{this.state.error}</p><button onClick={() => location.reload()}>Перезагрузить</button></main> : this.props.children; }
}
createRoot(document.getElementById('root')!).render(<ErrorBoundary><App/></ErrorBoundary>);
