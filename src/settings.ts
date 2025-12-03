import { createApp } from "vue";
import { createPinia } from "pinia";
import SettingsPage from "./pages/SettingsPage.vue";
import "./assets/styles.css";

const app = createApp(SettingsPage);
app.use(createPinia());
app.mount("#app");
