const channel = new BroadcastChannel("block-world-renderer-steel-workers");
const workers = [];

function waitForMessage(target, type) {
  return new Promise((resolve) => {
    target.addEventListener("message", function onMessage({ data }) {
      if (data == null || data.type !== type) return;
      target.removeEventListener("message", onMessage);
      resolve(data);
    });
  });
}

waitForMessage(self, "steel_worker_init").then(async (data) => {
  const pkg = await import(data.mainJs);
  await pkg.default(data.module, data.memory);
  pkg.steel_worker_entry(data.task);
});

export function enableSteelWorkers(mainJs, module, memory) {
  channel.onmessage = ({ data }) => {
    const worker = new Worker(import.meta.url, {
      type: "module",
      name: data.name,
    });
    worker.postMessage({
      type: "steel_worker_init",
      mainJs,
      module,
      memory,
      task: data.task,
    });
    workers.push(worker);
  };
}

export function requestSteelWorker(task, name) {
  channel.postMessage({ task, name });
}
