use anyhow::Context as _;
use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerVerificationKey;
use tracing::level_filters::LevelFilter;
use zkevm_test_harness::{
    compute_setups::{generate_base_layer_vks_and_proofs, generate_recursive_layer_vks_and_proofs},
    data_source::{in_memory_data_source::InMemoryDataSource, SetupDataSource},
    proof_wrapper_utils::{
        check_trusted_setup_file_existace, get_wrapper_setup_and_vk_from_scheduler_vk,
        WrapperConfig,
    },
};
use zksync_prover_fri_types::{
    circuit_definitions::{
        circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType,
        zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
    },
    ProverServiceDataKey,
};
use zksync_types::basic_fri_types::AggregationRound;
use zksync_vk_setup_data_server_fri::{
    get_base_path, get_round_for_recursive_circuit_type, save_base_layer_vk,
    save_finalization_hints, save_recursive_layer_vk, save_snark_vk,
};

#[cfg(test)]
mod tests;

fn save_vks(source: &dyn SetupDataSource) -> anyhow::Result<()> {
    for base_circuit_type in
        (BaseLayerCircuitType::VM as u8)..=(BaseLayerCircuitType::L1MessagesHasher as u8)
    {
        let vk = source.get_base_layer_vk(base_circuit_type).map_err(|err| {
            anyhow::anyhow!("No vk exist for circuit type: {base_circuit_type}: {err}")
        })?;
        save_base_layer_vk(vk).context("save_base_layer_vk()")?;
    }
    for leaf_circuit_type in (ZkSyncRecursionLayerStorageType::LeafLayerCircuitForMainVM as u8)
        ..=(ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8)
    {
        let vk = source
            .get_recursion_layer_vk(leaf_circuit_type)
            .map_err(|err| {
                anyhow::anyhow!("No vk exist for circuit type: {leaf_circuit_type}: {err}")
            })?;
        save_recursive_layer_vk(vk).context("save_recursive_layer_vk()")?;
    }
    save_recursive_layer_vk(
        source
            .get_recursion_layer_node_vk()
            .map_err(|err| anyhow::anyhow!("No vk exist for node layer circuit: {err}"))?,
    )
    .context("save_recursive_layer_vk")?;
    save_recursive_layer_vk(
        source
            .get_recursion_layer_vk(ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8)
            .map_err(|err| anyhow::anyhow!("No vk exist for scheduler circuit: {err}"))?,
    )
    .context("save_recursive_layer_vk")?;
    Ok(())
}

fn save_finalization_hints_using_source(source: &dyn SetupDataSource) -> anyhow::Result<()> {
    for base_circuit_type in
        (BaseLayerCircuitType::VM as u8)..=(BaseLayerCircuitType::L1MessagesHasher as u8)
    {
        let hint = source
            .get_base_layer_finalization_hint(base_circuit_type)
            .map_err(|err| {
                anyhow::anyhow!(
                    "No finalization_hint exist for circuit type: {base_circuit_type}: {err}"
                )
            })?
            .into_inner();
        let key = ProverServiceDataKey::new(base_circuit_type, AggregationRound::BasicCircuits);
        save_finalization_hints(key, &hint).context("save_finalization_hints()")?;
    }
    for leaf_circuit_type in (ZkSyncRecursionLayerStorageType::LeafLayerCircuitForMainVM as u8)
        ..=(ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8)
    {
        let hint = source
            .get_recursion_layer_finalization_hint(leaf_circuit_type)
            .map_err(|err| {
                anyhow::anyhow!(
                    "No finalization hint exist for circuit type: {leaf_circuit_type}: {err}"
                )
            })?
            .into_inner();
        let key = ProverServiceDataKey::new(
            leaf_circuit_type,
            get_round_for_recursive_circuit_type(leaf_circuit_type),
        );
        save_finalization_hints(key, &hint).context("save_finalization_hints()")?;
    }

    let node_hint = source
        .get_recursion_layer_node_finalization_hint()
        .map_err(|err| anyhow::anyhow!("No finalization hint exist for node layer circuit: {err}"))?
        .into_inner();
    save_finalization_hints(
        ProverServiceDataKey::new(
            ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
            AggregationRound::NodeAggregation,
        ),
        &node_hint,
    )
    .context("save_finalization_hints()")?;

    let scheduler_hint = source
        .get_recursion_layer_finalization_hint(
            ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
        )
        .map_err(|err| {
            anyhow::anyhow!("No finalization hint exist for scheduler layer circuit: {err}")
        })?
        .into_inner();
    save_finalization_hints(
        ProverServiceDataKey::new(
            ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            AggregationRound::Scheduler,
        ),
        &scheduler_hint,
    )
    .context("save_finalization_hints()")
}

fn generate_snark_vk(
    scheduler_vk: ZkSyncRecursionLayerVerificationKey,
    compression_mode: u8,
) -> anyhow::Result<()> {
    let config = WrapperConfig::new(compression_mode);

    let (_, vk) = get_wrapper_setup_and_vk_from_scheduler_vk(scheduler_vk, config);
    save_snark_vk(vk).context("save_snark_vk")
}

fn generate_vks() -> anyhow::Result<()> {
    // Start by checking the trusted setup existence.
    // This is used at the last step, but we want to fail early if user didn't configure everything
    // correctly.
    check_trusted_setup_file_existace();

    let mut in_memory_source = InMemoryDataSource::new();
    tracing::info!("Generating verification keys for Base layer.");
    generate_base_layer_vks_and_proofs(&mut in_memory_source)
        .map_err(|err| anyhow::anyhow!("Failed generating base vk's: {err}"))?;
    tracing::info!("Generating verification keys for Recursive layer.");
    generate_recursive_layer_vks_and_proofs(&mut in_memory_source)
        .map_err(|err| anyhow::anyhow!("Failed generating recursive vk's: {err}"))?;
    tracing::info!("Saving finalization hints.");
    save_finalization_hints_using_source(&in_memory_source)
        .context("save_finalization_hints_using_source()")?;
    tracing::info!("Saving circuit verification keys.");
    save_vks(&in_memory_source).context("save_vks()")?;

    // Generate snark VK
    let scheduler_vk = in_memory_source
        .get_recursion_layer_vk(ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8)
        .map_err(|err| anyhow::anyhow!("Failed to get scheduler vk: {err}"))?;
    tracing::info!("Generating verification keys for snark wrapper.");
    generate_snark_vk(scheduler_vk, 1).context("generate_snark_vk")
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    tracing::info!(
        "Generating verification keys and storing them inside {:?}",
        get_base_path()
    );
    generate_vks().context("generate_vks()")
}
