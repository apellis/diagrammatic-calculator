import { render } from 'solid-js/web';
import 'katex/dist/katex.min.css';
import './styles.css';
import { App } from './ui/App';
import { applyPatterns, applyTheme, loadTheme } from './ui/theme';

applyTheme(loadTheme());
applyPatterns();

render(() => <App />, document.getElementById('app')!);
