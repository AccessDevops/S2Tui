import { createApp } from "vue";
import { createPinia } from "pinia";
import PermissionsPage from "./pages/PermissionsPage.vue";
import "./assets/styles.css";

const app = createApp(PermissionsPage);
app.use(createPinia());
app.mount("#app");
