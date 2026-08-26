import '@fontsource/ibm-plex-mono/400.css';
import '@fontsource/ibm-plex-mono/500.css';
import { mount } from 'svelte';

import App from './App.svelte';
import './styles.css';

const target = document.getElementById('app');
if (target === null) {
  throw new Error('App root is missing');
}

mount(App, { target });
