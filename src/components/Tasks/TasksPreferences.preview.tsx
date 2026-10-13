/** Owned synthetic provider fixture. UI state uses the production uiStore and HTTP transport. */
import { render } from "solid-js/web";
import "../../global.css";
import "../../styles.css";
import { workItemProviderRegistry } from "../../plugins/workItemProviderRegistry";
import { repositoriesStore } from "../../stores/repositories";
import { uiStore } from "../../stores/ui";
import { listDemo } from "./list/demo";
import { TasksShell } from "./TasksShell";

workItemProviderRegistry.register(listDemo(12, "personal"));
workItemProviderRegistry.register(listDemo(12, "work"));
repositoriesStore.setActive("/tasks-memory-fixture");
await uiStore.hydrate();
const root = document.getElementById("root");
if (!root) throw new Error("Tasks fixture root missing");
render(() => <TasksShell />, root);
