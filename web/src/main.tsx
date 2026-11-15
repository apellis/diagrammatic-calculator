import { render } from 'solid-js/web';
import 'katex/dist/katex.min.css';
import './styles.css';
import { App } from './ui/App';
import { applyGhosts, applyPatterns, applyTheme, loadTheme } from './ui/theme';

applyTheme(loadTheme());
applyPatterns();
applyGhosts();

render(() => <App />, document.getElementById('app')!);
