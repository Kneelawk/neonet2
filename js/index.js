import("../pkg/index.js").catch(console.error).then(({start_neo_net}) => {
    start_neo_net("canvas");
});
