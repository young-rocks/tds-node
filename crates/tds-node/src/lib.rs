use halo2_proofs::halo2curves::pasta::{EqAffine, Fp};
use halo2_proofs::poly::commitment::ParamsProver;
use halo2_proofs::poly::ipa::commitment::ParamsIPA;
use halo2_proofs::SerdeFormat;
use movelang::argument::ScriptArguments;
use movelang::move_binary_format::file_format::{empty_script, CompiledScript};
use movelang::move_binary_format::CompiledModule;
use movelang::value::TypeTag;
use zkmove_vm::runtime::Runtime;
use zkmove_vm::state::StateStore;
use zkmove_vm_circuit::circuit::VmCircuit;

#[derive(Copy, Clone, Debug, Default)]
pub struct CircuitConfig {
    pub max_step_row: Option<usize>,
    pub stack_ops_num: Option<usize>,
    pub locals_ops_num: Option<usize>,
    pub global_ops_num: Option<usize>,
    pub max_frame_index: Option<usize>,
    pub max_locals_size: Option<usize>,
    pub max_stack_size: Option<usize>,
    pub word_size: Option<usize>,
}

impl From<CircuitConfig> for zkmove_vm_circuits::witness::CircuitConfig {
    fn from(
        CircuitConfig {
            max_step_row,
            stack_ops_num,
            locals_ops_num,
            global_ops_num,
            max_frame_index,
            max_locals_size,
            max_stack_size,
            word_size,
        }: CircuitConfig,
    ) -> Self {
        let mut config = zkmove_vm_circuits::witness::CircuitConfig::default()
            .max_step_row(max_step_row)
            .stack_ops_num(stack_ops_num)
            .locals_ops_num(locals_ops_num)
            .global_ops_num(global_ops_num)
            .word_size(word_size);
        if let Some(c) = max_frame_index {
            config = config.max_frame_index(c);
        }
        if let Some(c) = max_locals_size {
            config = config.max_locals_size(c);
        }
        if let Some(c) = max_stack_size {
            config = config.max_stack_size(c);
        }
        config
    }
}
#[derive(Clone, Default)]
pub struct DemoRunConfig {
    args: Option<ScriptArguments>,
    ty_args: Option<Vec<TypeTag>>,
}

#[derive(Clone, Debug)]
pub struct EntryFunctionConfig {
    entry_function: String, // TODO: replace it with struct
    demo_run_config: DemoRunConfig,
    circuit_config: CircuitConfig,
}

pub struct PublishModulesConfig {
    modules: Vec<Vec<u8>>,
    entry_function_config: Vec<EntryFunctionConfig>,
}

pub fn gen_vks(
    PublishModulesConfig {
        modules,
        entry_function_config,
    }: PublishModulesConfig,
) -> anyhow::Result<Vec<Vec<u8>>> {
    let rt = Runtime::<Fp>::new();
    let mut state = StateStore::new();
    let mut compiled_modules = Vec::default();
    for m in &modules {
        let m = CompiledModule::deserialize(m)?;
        compiled_modules.push(m.clone());
        state.add_module(m);
    }

    let mut vks = Vec::new();
    for EntryFunctionConfig {
        entry_function,
        demo_run_config,
        circuit_config,
    } in entry_function_config
    {
        let witness = rt
            .execute_script(
                empty_script(),
                compiled_modules.clone(),
                demo_run_config.ty_args.clone().unwrap_or_default(),
                None,
                demo_run_config.args.clone(),
                &mut state,
                circuit_config.into(),
            )
            .unwrap();

        let vm_circuit = VmCircuit { witness };
        let k = runtime.find_best_k(&vm_circuit, vec![])?;
        let params: ParamsIPA<EqAffine> = ParamsIPA::new(k);
        let (vk, _) = rt.setup_vm_circuit_ipa(&vm_circuit, &params)?;
        vks.push(vk.to_bytes(SerdeFormat::Processed));
    }
    Ok(vks)
}
