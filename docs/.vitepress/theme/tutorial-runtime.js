import { withBase } from 'vitepress';

let moduleRequest;

export function loadTutorial() {
  if (!moduleRequest) {
    const entry = withBase('/play/mirui-first-app.js');
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
