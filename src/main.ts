import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import './assets/main.css'

if (window.location.hash === '#/tray') {
  import('./components/TrayMenu.vue').then(({ default: TrayMenu }) =>
    createApp(TrayMenu).mount('#app'),
  )
} else {
  createApp(App).use(createPinia()).use(router).mount('#app')
}
