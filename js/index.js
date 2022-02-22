let cur_shutdown;

import("../pkg/index.js").catch(console.error).then(({start_neonet, stop_neonet}) => {
    cur_shutdown = stop_neonet;
    start_neonet("canvas-container", "canvas");
});
