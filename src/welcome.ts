import { createApp } from "vue";
import { createPinia } from "pinia";
import WelcomePage from "./pages/WelcomePage.vue";
import "./assets/styles.css";

const app = createApp(WelcomePage);
app.use(createPinia());
app.mount("#app");
