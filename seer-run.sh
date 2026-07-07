
### snowcap 服务器
cargo run -p snowcap_main -- export-seer-chain-gadget \
  -o /root/snowcap-workspace/seer/experiments/example_networks/ChainGadget

cargo run -p snowcap_main -- export-seer-difficult-gadget-repeated \
  -o /root/snowcap-workspace/seer/experiments/example_networks/DifficultGadgetRepeated

cargo run -p snowcap_main -- export-seer-variable-abilene-spec-complexity \
  -o /root/snowcap-workspace/seer/experiments/example_networks/VariableAbileneSpecComplexity

cargo run -p snowcap_main -- export-seer-topology-zoo-effectiveness \
  -o /root/snowcap-workspace/seer/experiments/topology_zoo_effectiveness \
  --topology-root eval_sigcomm2021/topology_zoo

### snowcap mac
cargo run -p snowcap_main -- export-seer-chain-gadget \
  -o /Users/liml/IdeaProjects/seer/experiments/example_networks/ChainGadget

cargo run -p snowcap_main -- export-seer-difficult-gadget-repeated \
  -o /Users/liml/IdeaProjects/seer/experiments/example_networks/DifficultGadgetRepeated

cargo run -p snowcap_main -- export-seer-variable-abilene-spec-complexity \
  -o /Users/liml/IdeaProjects/seer/experiments/example_networks/VariableAbileneSpecComplexity

cargo run -p snowcap_main -- export-seer-topology-zoo-effectiveness \
  -o /Users/liml/IdeaProjects/seer/experiments/topology_zoo_effectiveness \
  --topology-root eval_sigcomm2021/topology_zoo

