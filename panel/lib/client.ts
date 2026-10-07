export type Event = { request_id:string; sequence:number; stage:string; elapsed_ms:number; timestamp_ms:number; details:Record<string,unknown> };
export type BackendEvent = { request_id:string; service:string; operation:string; state:string; duration_ms:number|null;span_id?:string|null;parent_id?:string|null;elapsed_ms?:number|null };
export type Journey = { request_id:string; route:string; actor:string; started_ms:number; events:Event[]; backend_events:BackendEvent[]; truncated:boolean };
export type Verdict = { key:string; actor:string; route:string; reason:string; source:string; created_ms:number; expires_ms:number; request_id:string|null };
export type Cancellation = {action_id:string;request_id:string;route:string;state:string;reason:string;created_ms:number;expires_ms:number};
export type State = {
 queue:{capacity:number;waiting:number;timeout_ms:number;entered:number;completed:number;resumed:number;full:number;timed_out:number;unavailable:number;cancelled:number;total_us:number};
 configuration:{queue_capacity:number;queue_timeout_ms:number;threads:number;work_stealing:boolean;accept_tasks:number;upstream_keepalive_capacity:number;write_buffer_bytes:number;keepalive_seconds:number;scan_bytes:number;response_scan_bytes:number;journey:boolean;on_overload:string;feature_count:number;feature_version:number;route_count:number;routes:{name:string;path:string;host:string|null;methods:string[];upstream:string;capture:boolean;model:string;rate_limit_10s:number;deny:boolean;exact:boolean}[]};
 resources:{rss_bytes:number|null;threads:number|null;process_cpu_percent:number|null;logical_cpus:number;open_fds:number|null};
 storage:{enabled:boolean;healthy:boolean;committed_events:number;committed_batches:number;pressure_rejections:number;dropped_batches:number;write_retries:number;used_slots:number;capacity:number};
 version:string; config_version:string; uptime_ms:number; dropped_events:number;
 telemetry:{ total:number; active:number; completed:number; blocked:number; failed:number; recent:Event[]; latency_buckets:number[]; request_bytes:number; response_bytes:number };
 cache:{ score_hits:number; response_hits:number; response_bytes:number };
 policies:{persistence_admission:string;persistence_synchronous:string; model:string; max_in_flight:number; rate_limit_10s:number; decision_cache:boolean; response_cache:boolean;deny_ttl_ms:number;cancellable_routes:string[] };
 upstreams:{ name:string; policy:string; backends:{ address:string; healthy:boolean; active:number; latency_us:number; weight:number }[] }[];
 webhooks:{ pending:number; delivered:number; failed:number; overflow:number };
 analysis:null|{processing:number;journal_enabled:boolean;journal_healthy:boolean;journal_us:number;journal_failures:number;recovered:number;pressure_rejections:number; submitted:number; finished:number; dropped:number; expired:number; failed:number; total_us:number; last_us:number; queued:number; capacity:number };
 decisions:null|{ hits:number; misses:number; write_failures:number; cached_keys:number; queued:number; generation:number };
 journeys:{ total:number; items:Journey[] };
 model:null|{ model_version:string; input_schema?:string|null; precision?:string; parameter_count:number; source:string; deployment_ready:boolean; evaluation_notice?:string|null; artifact_sha256:string };
};
export async function api<T> (prefix:string, token:string, path:string, body?:unknown, signal?:AbortSignal):Promise<T> {
 const response=await fetch(prefix+path,{method:body===undefined?"GET":"POST",credentials:"omit",cache:"no-store",signal,
  headers:{Authorization:"Bearer "+token,...(body===undefined?{}:{"Content-Type":"application/json"})},
  body:body===undefined?undefined:JSON.stringify(body)});
 if(!response.ok) {
  const message:Record<number,string>={401:"The access token was not accepted.",403:"This token cannot perform that action.",409:"The gateway state changed. Refresh and review the action before trying again.",429:"The gateway is busy. Wait briefly before retrying.",503:"The gateway cannot complete this operation right now."};
  throw new Error(message[response.status]??"Request failed ("+response.status+").");
 }
 return await response.json() as T;
}
