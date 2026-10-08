//! One JSON-RPC endpoint for every API-driven widget in the process.
use crate::inputs::InputHub;
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

pub struct ApiServer {
    pub address: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}
impl ApiServer {
    pub fn start(address: SocketAddr, hub: InputHub) -> io::Result<Self> {
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (stop, stopped) = oneshot::channel();
        let worker = thread::Builder::new().name("widget-json-rpc".into()).spawn(move || {
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).expect("validated nonblocking listener");
                let router = Router::new().route("/rpc", post(endpoint)).layer(DefaultBodyLimit::max(65_536)).with_state(hub);
                tokio::select! {
                    result = axum::serve(listener, router) => { if let Err(error) = result { eprintln!("Widget API stopped: {error}"); } },
                    _ = stopped => {}
                }
            });
            runtime.shutdown_timeout(Duration::from_secs(1));
        })?;
        Ok(Self {
            address,
            stop: Some(stop),
            worker: Some(worker),
        })
    }
}
impl Drop for ApiServer {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub struct ApiManager {
    hub: InputHub,
    address: SocketAddr,
    force: bool,
    server: Option<ApiServer>,
    retry: Instant,
}
impl ApiManager {
    pub fn new(hub: InputHub, address: SocketAddr, force: bool) -> Self {
        Self {
            hub,
            address,
            force,
            server: None,
            retry: Instant::now(),
        }
    }
    pub fn ensure(&mut self) -> io::Result<()> {
        if self.server.is_none()
            && (self.force || !self.hub.api_names().is_empty())
            && Instant::now() >= self.retry
        {
            self.retry = Instant::now() + Duration::from_secs(2);
            self.server = Some(ApiServer::start(self.address, self.hub.clone())?);
        }
        Ok(())
    }
    pub fn status(&self) -> String {
        self.server
            .as_ref()
            .map(|s| format!("API: http://{}/rpc", s.address))
            .unwrap_or_else(|| "API: starts when an API input is configured".into())
    }
}

fn error(id: Value, code: i32, message: impl ToString) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message.to_string()}})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateParams {
    widget: String,
    values: NumericValues,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NumericValues {
    value: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GetParams {
    widget: String,
}
fn dispatch(hub: &InputHub, request: &Value) -> Option<Value> {
    let Some(object) = request.as_object() else {
        return Some(error(Value::Null, -32600, "Invalid Request"));
    };
    let id = object.get("id").cloned().unwrap_or(Value::Null);
    let id_valid = id.is_null() || id.is_string() || id.is_number();
    let method = object.get("method").and_then(Value::as_str);
    let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
    if object.get("jsonrpc") != Some(&json!("2.0"))
        || method.is_none()
        || !id_valid
        || !(params.is_object() || params.is_array())
    {
        return Some(error(
            if id_valid { id } else { Value::Null },
            -32600,
            "Invalid Request",
        ));
    }
    let result: Result<Value, (i32, String)> = (|| match method.unwrap() {
        "widget.update" => {
            let params: UpdateParams =
                serde_json::from_value(params).map_err(|e| (-32602, e.to_string()))?;
            let snapshot = hub
                .api_update(&params.widget, params.values.value)
                .map_err(input_error)?;
            Ok(json!({"widget":params.widget,"values":{"value":snapshot.value}}))
        }
        "widget.get" => {
            let params: GetParams =
                serde_json::from_value(params).map_err(|e| (-32602, e.to_string()))?;
            let snapshot = hub.api_get(&params.widget).map_err(input_error)?;
            Ok(json!({"widget":params.widget,"values":{"value":snapshot.value}}))
        }
        "widget.list" => {
            if params != json!({}) && params != json!([]) {
                return Err((-32602, "widget.list takes no parameters".into()));
            }
            Ok(json!(hub.api_names()))
        }
        _ => Err((-32601, "Method not found".into())),
    })();
    if !object.contains_key("id") {
        return None;
    }
    Some(match result {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err((code, message)) => error(id, code, message),
    })
}
fn input_error(e: io::Error) -> (i32, String) {
    (
        if e.kind() == io::ErrorKind::NotFound {
            -32001
        } else {
            -32602
        },
        e.to_string(),
    )
}
async fn endpoint(State(hub): State<InputHub>, body: Bytes) -> Response {
    let request: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return Json(error(Value::Null, -32700, "Parse error")).into_response(),
    };
    let response = if let Some(batch) = request.as_array() {
        if batch.is_empty() {
            Some(error(Value::Null, -32600, "Invalid Request"))
        } else {
            let results: Vec<_> = batch
                .iter()
                .filter_map(|request| dispatch(&hub, request))
                .collect();
            (!results.is_empty()).then_some(Value::Array(results))
        }
    } else {
        dispatch(&hub, &request)
    };
    response
        .map(|value| Json(value).into_response())
        .unwrap_or_else(|| StatusCode::NO_CONTENT.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inputs::{Binding, InputService, InputSource};
    use std::path::Path;
    #[test]
    fn manager_starts_lazily_and_reuses_one_server_for_registry_changes() {
        let mut service = InputService::new(Path::new("."));
        let mut manager = ApiManager::new(service.hub(), "127.0.0.1:0".parse().unwrap(), false);
        manager.ensure().unwrap();
        assert!(manager.server.is_none());
        let binding = Binding {
            name: "first".into(),
            source: Some(InputSource::Api),
            initial: 0.0,
        };
        service.reconcile(vec![binding.clone()]).unwrap();
        manager.ensure().unwrap();
        let address = manager.server.as_ref().unwrap().address;
        service
            .reconcile(vec![
                binding,
                Binding {
                    name: "second".into(),
                    source: Some(InputSource::Api),
                    initial: 0.0,
                },
            ])
            .unwrap();
        manager.ensure().unwrap();
        assert_eq!(manager.server.as_ref().unwrap().address, address);
        service.reconcile(vec![]).unwrap();
        manager.ensure().unwrap();
        assert_eq!(manager.server.as_ref().unwrap().address, address);
    }

    #[test]
    fn rpc_routes_by_name_validates_payloads_and_handles_notifications() {
        let mut service = InputService::new(Path::new("."));
        service
            .reconcile(vec![
                Binding {
                    name: "one".into(),
                    source: Some(InputSource::Api),
                    initial: 0.0,
                },
                Binding {
                    name: "two".into(),
                    source: Some(InputSource::Api),
                    initial: 5.0,
                },
            ])
            .unwrap();
        let hub = service.hub();
        let request = json!({"jsonrpc":"2.0", "id":1, "method":"widget.update", "params":{"widget":"one", "values":{"value":50}}});
        assert_eq!(
            dispatch(&hub, &request).unwrap()["result"]["values"]["value"],
            50.0
        );
        assert_eq!(hub.snapshot("two").unwrap().value, 5.0);
        let mut notification = request.clone();
        notification.as_object_mut().unwrap().remove("id");
        assert!(dispatch(&hub, &notification).is_none());
        let mut bad = request.clone();
        bad["params"]["values"] = json!({"value":"wrong"});
        assert_eq!(dispatch(&hub, &bad).unwrap()["error"]["code"], -32602);
        bad = request.clone();
        bad["params"]["widget"] = json!("missing");
        assert_eq!(dispatch(&hub, &bad).unwrap()["error"]["code"], -32001);
        assert_eq!(
            dispatch(
                &hub,
                &json!({"jsonrpc":"2.0","id":true,"method":"widget.list"})
            )
            .unwrap()["error"]["code"],
            -32600
        );
        assert_eq!(
            dispatch(&hub, &json!({"jsonrpc":"2.0","id":2,"method":"missing"})).unwrap()["error"]
                ["code"],
            -32601
        );
        service.reconcile(vec![]).unwrap();
        assert!(hub.api_update("one", 10.0).is_err());
    }
    #[test]
    fn single_server_releases_listener_on_drop() {
        let server = ApiServer::start("127.0.0.1:0".parse().unwrap(), InputHub::default()).unwrap();
        let address = server.address;
        assert!(TcpListener::bind(address).is_err());
        drop(server);
        assert!(TcpListener::bind(address).is_ok());
    }
}
