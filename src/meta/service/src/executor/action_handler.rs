// Copyright 2021 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;

use common_meta_api::KVApi;
use common_meta_client::MetaGrpcReadReq;
use common_meta_client::MetaGrpcWriteReq;
use common_meta_client::RequestFor;
use common_meta_types::protobuf::RaftReply;
use common_meta_types::MetaError;
use common_meta_types::TxnReply;
use common_meta_types::TxnRequest;

use crate::meta_service::MetaNode;
use crate::metrics::incr_meta_metrics_meta_request_result;

pub struct ActionHandler {
    /// The raft-based meta data entry.
    pub(crate) meta_node: Arc<MetaNode>,
}

#[async_trait::async_trait]
pub trait RequestHandler<T>: Sync + Send
where T: RequestFor
{
    async fn handle(&self, req: T) -> Result<T::Reply, MetaError>;
}

impl ActionHandler {
    pub fn create(meta_node: Arc<MetaNode>) -> Self {
        ActionHandler { meta_node }
    }

    pub async fn execute_write(&self, action: MetaGrpcWriteReq) -> RaftReply {
        // To keep the code IDE-friendly, we manually expand the enum variants and dispatch them one by one

        match action {
            MetaGrpcWriteReq::UpsertKV(a) => {
                let r = self.meta_node.upsert_kv(a).await;
                incr_meta_metrics_meta_request_result(r.is_ok());
                RaftReply::from(r)
            }
        }
    }

    pub async fn execute_read(&self, action: MetaGrpcReadReq) -> RaftReply {
        // To keep the code IDE-friendly, we manually expand the enum variants and dispatch them one by one

        match action {
            MetaGrpcReadReq::GetKV(a) => {
                let r = self.meta_node.get_kv(&a.key).await;
                incr_meta_metrics_meta_request_result(r.is_ok());
                RaftReply::from(r)
            }
            MetaGrpcReadReq::MGetKV(a) => {
                let r = self.meta_node.mget_kv(&a.keys).await;
                incr_meta_metrics_meta_request_result(r.is_ok());
                RaftReply::from(r)
            }
            MetaGrpcReadReq::ListKV(a) => {
                let r = self.meta_node.prefix_list_kv(&a.prefix).await;
                incr_meta_metrics_meta_request_result(r.is_ok());
                RaftReply::from(r)
            }
        }
    }

    pub async fn execute_txn(&self, req: TxnRequest) -> TxnReply {
        let ret = self.meta_node.transaction(req).await;
        incr_meta_metrics_meta_request_result(ret.is_ok());

        match ret {
            Ok(resp) => resp,
            Err(err) => TxnReply {
                success: false,
                error: serde_json::to_string(&err).expect("fail to serialize"),
                responses: vec![],
            },
        }
    }
}
