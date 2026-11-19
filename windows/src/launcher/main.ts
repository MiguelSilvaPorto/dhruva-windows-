import { mount } from 'svelte';
import Launcher from './Launcher.svelte';
import '../styles/global.css';

const target = document.getElementById('app');
if (target) {
    mount(Launcher, { target });
}
