use anyhow::anyhow;
use halo2_proofs::halo2curves::bn256::Fr;
use move_core_types::resolver::ModuleResolver;
use movelang::argument::{
    parse_transaction_argument, parse_type_tags, Identifier, ScriptArguments,
};
use movelang::move_binary_format::access::ModuleAccess;
use movelang::move_binary_format::file_format::FunctionDefinitionIndex;
use movelang::move_binary_format::CompiledModule;
use movelang::value::ModuleId;
use zkmove_vm::runtime::Runtime;
use zkmove_vm::state::StateStore;
use zkmove_vm_circuit::witness::Witness;

use agger_contract_types::UserQuery;

use crate::vk_generator::VerificationParameters;

pub fn witness(
    query: UserQuery,
    modules: Vec<Vec<u8>>,
    vp: &VerificationParameters,
) -> anyhow::Result<Witness<Fr>> {
    let mut state = StateStore::new();
    let mut compiled_modules = Vec::default();
    for m in &modules {
        let m = CompiledModule::deserialize(m)?;
        compiled_modules.push(m.clone());
        state.add_module(m);
        // todo: replace it with execute entry_function
    }
    let rt = Runtime::<Fr>::new();
    let ty_args = query
        .query
        .ty_args
        .into_iter()
        .map(|t| {
            let s = String::from_utf8(t)?;
            let mut ts = parse_type_tags(s.as_str())?;
            ts.pop().ok_or_else(|| anyhow!("parse type arg failure"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let args = query
        .query
        .args
        .into_iter()
        .map(|arg| {
            let s = String::from_utf8(arg)?;
            let ta = parse_transaction_argument(s.as_str())?;
            Ok(ta)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let entry_module_address =
        move_core_types::account_address::AccountAddress::from_bytes(&query.query.module_address)?;
    let entry_module_name = Identifier::from_utf8(query.query.module_name.clone())?;
    let entry_module_id = ModuleId::new(entry_module_address, entry_module_name);
    let entry_module = CompiledModule::deserialize(
        &state
            .get_module(&entry_module_id)?
            .ok_or(anyhow!("cannot find module {}", &entry_module_id))?,
    )?;

    let entry_function_name = entry_module.identifier_at(
        entry_module
            .function_handle_at(
                entry_module
                    .function_def_at(FunctionDefinitionIndex::new(query.query.function_index))
                    .function,
            )
            .name,
    );

    let traces = rt
        .execute_entry_function(
            &entry_module_id,
            entry_function_name,
            ty_args.clone(),
            None,
            if args.is_empty() {
                None
            } else {
                Some(ScriptArguments::new(args.clone()))
            },
            &mut state,
        )
        .unwrap();

    let witness = rt.process_execution_trace(
        ty_args,
        None,
        Some((&entry_module_id, entry_function_name)),
        compiled_modules.clone(),
        traces,
        bcs::from_bytes(&vp.config)?,
    )?;
    Ok(witness)
}
