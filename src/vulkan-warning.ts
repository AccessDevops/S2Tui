import { createApp } from "vue";
import { createPinia } from "pinia";
import VulkanWarningPage from "./pages/VulkanWarningPage.vue";
import "./assets/styles.css";

const app = createApp(VulkanWarningPage);
app.use(createPinia());
app.mount("#app");
