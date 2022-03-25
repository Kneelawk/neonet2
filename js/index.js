let cur_flow;

function render_cur_flow() {
    let flow = cur_flow;
    if (flow) {
        flow.render().then(() => {
            window.requestAnimationFrame(render_cur_flow);
        });
    }
}

function stop_cur_flow() {
    let flow = cur_flow;
    cur_flow = null;
    if (flow) {
        flow.free();
    }
}

function resize_cur_flow() {
    let width = window.innerWidth;
    let height = window.innerHeight;

    let flow = cur_flow;
    if (flow) {
        flow.resize(width, height);
    }
}

import("../pkg/index.js").catch(console.error).then(({start_neonet}) => {
    start_neonet("canvas-container", "canvas").then((flow) => {
        cur_flow = flow;
        window.requestAnimationFrame(render_cur_flow);

        // setTimeout(() => {
        //     stop_cur_flow();
        // }, 5000);
    });
});

window.addEventListener("resize", resize_cur_flow);
