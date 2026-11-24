import { withBase } from 'vitepress';

export const workspaceSettings = Symbol('tutorial workspace');
let moduleRequest;

export function loadTutorial() {
  if (!moduleRequest) {
    const entry = new URL(withBase('/play/mirui-first-app.js'), window.location.origin).href;
    moduleRequest = import(/* @vite-ignore */ entry).then(async module => {
      await module.default();
      return module;
    }).catch(error => {
      moduleRequest = undefined;
      throw error;
    });
  }
  return moduleRequest;
}
