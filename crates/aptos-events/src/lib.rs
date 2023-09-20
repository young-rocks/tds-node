use std::time::Duration;

use aptos_sdk::move_types::identifier::Identifier;
use aptos_sdk::rest_client::aptos_api_types::{
    EntryFunctionId, IdentifierWrapper, MoveModuleId, VersionedEvent, ViewRequest,
};
use aptos_sdk::rest_client::error::RestError;
pub use aptos_sdk::rest_client::AptosBaseUrl;
use aptos_sdk::rest_client::Client;
pub use aptos_sdk::types::account_address::AccountAddress as AptosAccountAddress;
use async_stream::{stream};
use futures_core::Stream;

use tokio::time::sleep;

use agger_contract_types::*;

#[derive(Clone, Debug)]
pub struct AggerQueryManager {
    client: Client,
    param: AggerQueryParam,
}

#[derive(Clone, Debug)]
pub struct AggerQueryParam {
    pub aggger_address: AptosAccountAddress,
}

type AptosResult<T> = Result<T, RestError>;

impl AggerQueryManager {
    pub fn new(aptos_url: AptosBaseUrl, param: AggerQueryParam) -> Self {
        Self {
            client: Client::builder(aptos_url).build(),
            param,
        }
    }
    pub async fn prepare_modules(
        &self,
        UserQuery { query, version, .. }: &UserQuery,
    ) -> AptosResult<Vec<Vec<u8>>> {
        let req = ViewRequest {
            function: EntryFunctionId {
                module: MoveModuleId {
                    address: self.param.aggger_address.into(),
                    name: IdentifierWrapper(Identifier::new(AGGER_QUERY_MODULE_NAME).unwrap()),
                },
                name: IdentifierWrapper(Identifier::new(AGGER_QUERY_FUNC_NAME_GET_MODULE).unwrap()),
            },
            type_arguments: vec![],
            arguments: vec![
                serde_json::to_value(query.module_address.clone()).unwrap(),
                serde_json::to_value(query.module_name.clone()).unwrap(),
            ],
        };
        let response = self
            .client
            .view(&req, Some(*version))
            .await?
            .into_inner()
            .pop();
        let module_bytes: Vec<_> = response
            .map(serde_json::from_value)
            .transpose()?
            .expect("view get_module should return one value");
        // TODO: fetch deps if any
        Ok(vec![module_bytes])
    }

    ///return (config, vk,param) for a user query
    pub async fn get_vk_for_query(
        &self,
        UserQuery { query, version, .. }: &UserQuery,
    ) -> AptosResult<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let reqs = vec![
            ViewRequest {
                function: EntryFunctionId {
                    module: MoveModuleId {
                        address: self.param.aggger_address.into(),
                        name: IdentifierWrapper(Identifier::new(AGGER_QUERY_MODULE_NAME).unwrap()),
                    },
                    name: IdentifierWrapper(
                        Identifier::new(AGGER_QUERY_FUNC_NAME_GET_CONFIG).unwrap(),
                    ),
                },
                type_arguments: vec![],
                arguments: vec![
                    serde_json::to_value(query.module_address.clone()).unwrap(),
                    serde_json::to_value(query.module_name.clone()).unwrap(),
                    serde_json::to_value(query.function_index).unwrap(),
                ],
            },
            ViewRequest {
                function: EntryFunctionId {
                    module: MoveModuleId {
                        address: self.param.aggger_address.into(),
                        name: IdentifierWrapper(Identifier::new(AGGER_QUERY_MODULE_NAME).unwrap()),
                    },
                    name: IdentifierWrapper(Identifier::new(AGGER_QUERY_FUNC_NAME_GET_VK).unwrap()),
                },
                type_arguments: vec![],
                arguments: vec![
                    serde_json::to_value(query.module_address.clone()).unwrap(),
                    serde_json::to_value(query.module_name.clone()).unwrap(),
                    serde_json::to_value(query.function_index).unwrap(),
                ],
            },
            ViewRequest {
                function: EntryFunctionId {
                    module: MoveModuleId {
                        address: self.param.aggger_address.into(),
                        name: IdentifierWrapper(Identifier::new(AGGER_QUERY_MODULE_NAME).unwrap()),
                    },
                    name: IdentifierWrapper(
                        Identifier::new(AGGER_QUERY_FUNC_NAME_GET_PARAM).unwrap(),
                    ),
                },
                type_arguments: vec![],
                arguments: vec![
                    serde_json::to_value(query.module_address.clone()).unwrap(),
                    serde_json::to_value(query.module_name.clone()).unwrap(),
                    serde_json::to_value(query.function_index).unwrap(),
                ],
            },
        ];
        let mut reqs: Vec<_> = reqs
            .iter()
            .map(|req| self.client.view(req, Some(*version)))
            .collect();

        let (param, vk, config) = tokio::try_join!(
            reqs.pop().unwrap(),
            reqs.pop().unwrap(),
            reqs.pop().unwrap()
        )?;
        let param: Vec<u8> = param
            .into_inner()
            .pop()
            .map(serde_json::from_value)
            .transpose()?
            .expect("view get_param return value");
        let vk: Vec<u8> = vk
            .into_inner()
            .pop()
            .map(serde_json::from_value)
            .transpose()?
            .expect("view get_vk return value");
        let config: Vec<u8> = config
            .into_inner()
            .pop()
            .map(serde_json::from_value)
            .transpose()?
            .expect("view get_config return value");

        Ok((config, vk, param))
    }

    pub fn get_query_stream(self) -> impl Stream<Item = AptosResult<UserQuery>> {
        stream! {
            let mut cur = 0;
            loop {
                let event = self.get_event(cur).await;
                match event {
                    Ok(Some(evt)) => {
                        let q = self.handle_new_query_event(evt).await;
                        yield q;
                        cur += 1;
                    }
                    Ok(None) => {
                        sleep(Duration::from_secs(30)).await;
                    }
                    Err(e) => {
                        yield Err(e)
                    }
                }
                // if let Some(evt) = event {
                //
                // } else {
                //     sleep(Duration::from_secs(30)).await;
                // }
            }
        }
    }
    async fn handle_new_query_event(&self, event: VersionedEvent) -> AptosResult<UserQuery> {
        let new_query_event: NewQueryEvent = serde_json::from_value(event.data)?;
        let version = event.version.0;
        let response = self
            .client
            .get_account_resource_at_version_bcs(
                new_query_event.user,
                format!(
                    "{:#x}::{}::{}",
                    self.param.aggger_address,
                    AGGER_QUERY_MODULE_NAME,
                    AGGER_QUERY_QUERIES_STRUCT_NAME
                )
                .as_str(),
                version,
            )
            .await?;
        let queries: Queries = response.into_inner();

        let query = self
            .client
            .get_table_item_bcs_at_version::<_, Query>(
                queries.queries.inner.handle,
                "u64",
                format!(
                    "{:#x}::{}::{}",
                    self.param.aggger_address,
                    AGGER_QUERY_MODULE_NAME,
                    AGGER_QUERY_QUERY_STRUCT_NAME
                )
                .as_str(),
                new_query_event.id,
                version,
            )
            .await?;
        Ok(UserQuery {
            version,
            sequence_number: event.sequence_number.0,
            id: new_query_event.id,
            user: new_query_event.user,
            query: query.into_inner(),
        })
    }
    async fn get_event(&self, at: u64) -> AptosResult<Option<VersionedEvent>> {
        let response = self
            .client
            .get_account_events(
                self.param.aggger_address,
                format!(
                    "{:#x}::{}::{}",
                    self.param.aggger_address,
                    AGGER_QUERY_MODULE_NAME,
                    AGGER_QUERY_EVENT_HANDLES_STRUCT_NAME
                )
                .as_str(),
                AGGER_QUERY_FIELD_NAME_NEW_EVENT_HANDLE,
                Some(at),
                Some(1),
            )
            .await?;
        let mut events = response.into_inner();
        Ok(events.pop())
    }
}
