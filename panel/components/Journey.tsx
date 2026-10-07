"use client";
import type {Event,Journey as Active} from "../lib/client";
import {Dialog,Empty,Icon} from "./ui";
export function Journey({id,events,active,close,block,canBlock,cancel,canCancel,error,loading,busy}:{id:string;events:Event[];active?:Active;close:()=>void;block:()=>void;canBlock:boolean;cancel:()=>void;canCancel:boolean;error:string;loading:boolean;busy:boolean}){
 const ordered=[...events].sort((a,b)=>a.timestamp_ms-b.timestamp_ms||a.sequence-b.sequence);
 const analysis=ordered.findLast(event=>event.stage==="analyzed");
 const reported=ordered.filter(event=>event.stage==="backend_reported").map(event=>event.details as Active["backend_events"][number]);
 const backend=active?.backend_events??(reported.length?reported:(analysis?.details.backend_events as Active["backend_events"]|undefined)??[]);
 const latest=ordered.at(-1);
 const scores=analysis?.details.component_scores as {content:number;journey:number}|undefined;
 const evidence=analysis?.details.model_inputs as {schema?:string;request_bytes:number;response_bytes:number;backend_events:number;request_truncated:boolean;response_truncated:boolean;events_truncated:boolean;response_available:boolean}|undefined;
 const inputs=evidence?.schema==="byte-journey-v1"?evidence:undefined;
 return <Dialog wide close={close} label="Request journey"><div className="detail">
 <div className="drawer-head"><div><span className="eyebrow">REQUEST EXPLORER</span><h2>Execution timeline</h2></div><button className="icon-button" aria-label="Close details" onClick={close}><Icon name="close"/></button></div>
 <div className="request-summary"><span className="badge violet">{active?"In flight":latest?.stage.replaceAll("_"," ")??"Loading"}</span><span className="mono">{latest?.elapsed_ms??0} ms</span></div>
 <p className="mono break request-id">request_id: {id}</p>
 {error&&<div className="callout danger" role="alert"><Icon name="alert"/><p>{error} Retained details may be incomplete.</p></div>}
 {loading&&!ordered.length?<div className="skeleton detail-skeleton" aria-label="Loading journey"/>:<ol className="timeline">{ordered.filter(event=>event.stage!=="backend_reported").map((event,index)=><li key={event.sequence+"-"+index}><span className={"timeline-icon "+(event.stage==="analyzed"?"violet":["blocked","rejected","failed"].includes(event.stage)?"danger":"")}><Icon name={event.stage==="analyzed"?"analysis":["blocked","rejected","failed"].includes(event.stage)?"shield":"check"} size={14}/></span><div><div className="timeline-label"><b>{event.stage.replaceAll("_"," ")}</b><span className="mono">{event.elapsed_ms.toLocaleString()} ms</span></div><small>{new Date(event.timestamp_ms).toLocaleTimeString()}</small>{!!event.details.reason&&<p>{String(event.details.reason)}</p>}{event.stage==="analyzed"&&<p>{String(event.details.action)} · risk {event.details.risk_score==null?"unavailable":(Number(event.details.risk_score)*100).toFixed(1)+"%"}</p>}</div></li>)}</ol>}
 {!loading&&!ordered.length&&<Empty title="No retained events">This journey may have expired from retention or was not captured.</Empty>}
 <section className="journey-section"><h3><Icon name="upstream" size={16}/> Backend operations</h3>{backend.length?backend.map((event,index)=><div className="backend-operation" key={index}><div><strong>{event.service}</strong><small>{event.operation}</small></div><span className="badge">{event.state}</span>{event.duration_ms!=null&&<span className="mono">{event.duration_ms} ms</span>}</div>):<p className="muted">No backend operations reported. Connect instrumentation to see execution inside your services.</p>}</section>
 {inputs&&<section className="journey-section"><h3><Icon name="analysis" size={16}/> Model evidence</h3>
 <p className="muted">{inputs.request_bytes} request bytes · {inputs.response_available?inputs.response_bytes+" response bytes":"response text unavailable"} · {inputs.backend_events} backend events</p>
 {(inputs.request_truncated||inputs.response_truncated||inputs.events_truncated)&&<p className="muted">Input budgets truncated part of this journey.</p>}
 {scores&&<p>Content signal {inputs.request_bytes?(scores.content*100).toFixed(2)+"%":"unavailable"} · Journey signal {inputs.backend_events?(scores.journey*100).toFixed(2)+"%":"unavailable"}</p>}
 <p className="muted">Research signals, not calibrated probabilities of compromise. Raw request and response text is not retained.</p></section>}
 <details><summary>Inspect event data</summary><pre>{JSON.stringify(ordered,null,2)}</pre></details>
 <div className="drawer-actions"><div className="callout info"><Icon name="shield"/><p>Late decisions apply to future admissions. Backend cancellation requires explicit cooperation.</p></div><button className="button primary" disabled={!canBlock||busy||loading} onClick={block}><Icon name="shield" size={16}/>Block future requests</button>{canCancel&&<button className="button secondary" disabled={busy} onClick={cancel}>Request safe backend cancellation</button>}</div>
 </div></Dialog>;
}
