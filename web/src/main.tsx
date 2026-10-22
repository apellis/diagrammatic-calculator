import { render } from 'solid-js/web';
import 'katex/dist/katex.min.css';
import './styles.css';
import { App } from './ui/App';

render(() => <App />, document.getElementById('app')!);
