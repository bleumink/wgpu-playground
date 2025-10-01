import init, {init_worker} from "./pkg/wgpu_web.js";

init().then(() => {    
    init_worker();
});