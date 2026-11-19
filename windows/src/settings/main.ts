import { mount } from 'svelte';
import App from './App.svelte';
import '../styles/global.css';
import '../styles/settings.css';

const target = document.getElementById('app');
if (target) {
    mount(App, { target });
}
