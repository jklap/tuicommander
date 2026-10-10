import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
export default defineConfig({
    plugins: [{
        name: "recorded-mobile-transport", enforce: "pre",
        resolveId(id, importer) {
            if (id === "../../transport" && importer?.endsWith("/mobile/components/OutputView.tsx")) return "\0mobile-replay";
        },
        load(id) {
            if (id === "\0mobile-replay") return `export async function subscribePty(_id, _data, _exit, options) {
                window.mobileReplay = options;
                return Object.assign(() => {}, { pause() {}, resume() {} });
            }`;
        },
    }, solid()],
    server: { host: "127.0.0.1", port: 14381, strictPort: true },
});
