"use client";
import {useCallback,useEffect,useRef,useState} from "react";
import {api,type Event,type Verdict} from "../lib/client";
import {useRuntime} from "../lib/runtime";
import {TrafficChart,LatencyChart,Sparkline} from "./Charts";
import {Journey} from "./Journey";
import {Shell,type View} from "./Shell";
import {Requests} from "./Requests";
import {Decisions,Diagnostics,Upstreams} from "./Operations";
import {Queue} from "./Queue";
import {Configuration} from "./Configuration";
import {ActionForm} from "./ActionForm";
import {Icon,Panel,Toast,format,type Notice} from "./ui";
const titles:Record<View,[string,string]>={
 Overview:["Gateway overview","A clear view of traffic, protection and the services behind it."],
 Requests:["Request explorer","Trace each captured journey. Understand what happened and why."],
 Decisions:["Protection & decisions","Review durable restrictions and coordinate safe backend actions."],
 Upstreams:["Upstream infrastructure","See where requests go and how each backend responds."],
 Analysis:["Intelligence & capture","Understand coverage, model activity and persistence health."],
 Configuration:["Configuration","One Lua file. Explicit policies. A consistent runtime."]
};
export function Console(){
 const [token,setToken]=useState(""),[draft,setDraft]=useState(""),[prefix,setPrefix]=useState(""),[bootError,setBootError]=useState("");
 const [view,setView]=useState<View>("Overview"),[dark,setDark]=useState(false),[paused,setPaused]=useState(false),[windowSize,setWindowSize]=useState(60);
 const [notice,setNotice]=useState<Notice|null>(null),[selected,setSelected]=useState(""),[events,setEvents]=useState<Event[]>([]),[detailError,setDetailError]=useState(""),[loadingDetail,setLoadingDetail]=useState(false),[busy,setBusy]=useState(false);
 const [intent,setIntent]=useState<{kind:"block"|"revoke"|"cancel";verdict?:Verdict}|null>(null);
 const actionController=useRef<AbortController|null>(null);
 const runtime=useRuntime(token,prefix,paused);
 const {state,verdicts,cancellations,points,error,updated}=runtime;
 const dismiss=useCallback(()=>setNotice(null),[]);
 const notify=(text:string,kind:Notice["kind"]="success")=>setNotice({text,kind,id:Date.now()});
 useEffect(()=>{const controller=new AbortController();fetch("/aegisx-bootstrap.json",{cache:"no-store",signal:controller.signal}).then(response=>{if(!response.ok)throw new Error("Gateway bootstrap is unavailable.");return response.json();})
 .then(value=>{if(typeof value.api_prefix!=="string"||!/^\/[a-zA-Z0-9/_-]+$/.test(value.api_prefix))throw new Error("Invalid gateway API configuration.");setPrefix(value.api_prefix);})
 .catch(error=>{if(!controller.signal.aborted)setBootError(error.message);});return()=>controller.abort();},[]);
 useEffect(()=>{if(!selected||!token)return;const controller=new AbortController();setLoadingDetail(true);
 api<{events:Event[]}>(prefix,token,"/requests/"+selected,undefined,AbortSignal.any([controller.signal,AbortSignal.timeout(5000)]))
 .then(value=>{if(controller.signal.aborted)return;setEvents(value.events);setDetailError("");}).catch(error=>{if(!controller.signal.aborted)setDetailError(error.message);}).finally(()=>{if(!controller.signal.aborted)setLoadingDetail(false);});
 return()=>controller.abort();},[selected,token,prefix,updated]);
 useEffect(()=>{function key(event:KeyboardEvent){if(event.key!=="/"||event.ctrlKey||event.metaKey||event.altKey||document.querySelector("dialog[open]")||["INPUT","TEXTAREA","SELECT"].includes((event.target as HTMLElement).tagName))return;event.preventDefault();setView("Requests");setTimeout(()=>document.querySelector<HTMLInputElement>('[aria-label="Search requests"]')?.focus(),0);}window.addEventListener("keydown",key);return()=>window.removeEventListener("keydown",key);},[]);
 useEffect(()=>()=>actionController.current?.abort(),[]);
 function disconnect(){actionController.current?.abort();setToken("");setDraft("");setSelected("");setEvents([]);setIntent(null);setNotice(null);setPaused(false);setBusy(false);}
 async function action(path:string,body:unknown,message:string){
  if(busy)return false;setNotice(null);setBusy(true);const controller=new AbortController();actionController.current=controller;
  try{await api(prefix,token,path,body,AbortSignal.any([controller.signal,AbortSignal.timeout(10000)]));if(controller.signal.aborted)return false;notify(message);runtime.refresh();return true;}
  catch(error){if(!controller.signal.aborted)notify((error as Error).name==="TimeoutError"?"The response timed out. The action may have committed; refresh before retrying.":(error as Error).message,"error");return false;}
  finally{if(actionController.current===controller){setBusy(false);actionController.current=null;}}
 }
 function select(id:string){setEvents([]);setDetailError("");setSelected(id);}
 const selectedActive=state?.journeys.items.find(item=>item.request_id===selected);
 const selectedEvent=events.find(event=>event.details.actor&&event.details.route)??state?.telemetry.recent.find(event=>event.request_id===selected&&event.details.actor);
 const route=String(selectedEvent?.details.route??selectedActive?.route??"");
 async function submit(reason:string,ttl:number){
  if(!state||!intent)return false;
  if(intent.kind==="revoke")return action("/blocks/revoke",{key:intent.verdict?.key},"Decision revoked. Cached state was updated after the database commit.");
  if(intent.kind==="cancel")return action("/cancellations",{request_id:selected,route},"Cancellation requested. Only the backend can confirm a safe outcome.");
  return action("/blocks",{config_version:state.config_version,request_id:selected,actor:selectedEvent?.details.actor,route,ttl_ms:ttl,reason},"Future admissions blocked for "+ttl/1000+" seconds. The active request is unchanged.");
 }
 function exportSnapshot(){if(!state)return;const blob=new Blob([JSON.stringify({exported_at:new Date().toISOString(),state},null,2)],{type:"application/json"});const url=URL.createObjectURL(blob);const link=document.createElement("a");link.href=url;link.download="aegisx-runtime.json";link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
 const latest=points.at(-1),failed=state?.telemetry.failed??0,total=state?.telemetry.total??0;
 return <Shell view={view} navigate={setView} online={!!state&&!error} paused={paused} connected={!!token} version={state?.version} dark={dark} toggleTheme={()=>setDark(value=>!value)} active={state?.telemetry.active??0} disconnect={disconnect}>
 <header className="page-header"><div><div className="eyebrow">YOUR INFRASTRUCTURE, IN FOCUS</div><h1>{titles[view][0]}</h1><p>{titles[view][1]}</p></div>{state&&<div className="header-actions"><button className="button secondary" onClick={()=>setPaused(value=>!value)}><Icon name={paused?"play":"pause"} size={15}/>{paused?"Resume live":"Pause live"}</button><button className="button secondary" onClick={exportSnapshot}><Icon name="download" size={15}/>Export snapshot</button></div>}</header>
 {bootError&&<div className="callout danger" role="alert"><Icon name="alert"/><div><strong>Gateway configuration unavailable</strong><p>{bootError} Reload this page after the control service is ready.</p></div></div>}
 {!token&&<div className="connect-layout"><section className="connect-story"><span className="connect-emblem"><Icon name="shield" size={38}/></span><span className="eyebrow">LOCAL BY DESIGN</span><h2>Your gateway.<br/>A clearer perspective.</h2><p>Follow requests, understand decisions and stay in control of your infrastructure.</p><div className="connect-features"><span><Icon name="requests"/>Live request journeys</span><span><Icon name="analysis"/>Background intelligence</span><span><Icon name="database"/>Local, explicit policies</span></div></section><form className="connect-form" onSubmit={event=>{event.preventDefault();setToken(draft);setDraft("");}}><span className="tag">OPERATOR ACCESS</span><h2>Connect to your gateway</h2><p>Use the admin token configured for this local instance.</p><label>Access token<input aria-label="Access token" type="password" autoComplete="off" spellCheck={false} value={draft} onChange={event=>setDraft(event.target.value)} required minLength={32} placeholder="Enter your admin token"/></label><button className="button primary" disabled={!prefix||!draft}><Icon name="key" size={16}/>Connect</button><small><Icon name="shield" size={14}/>Your token stays in this tab’s memory.</small></form></div>}
 {error&&<div className="callout danger" role="alert"><Icon name="alert"/><div><strong>Connection interrupted</strong><p>{error} {state?"Displayed data may be stale.":"Check your token and gateway availability."}</p></div><button className="button secondary small" onClick={disconnect}>Reconnect</button></div>}
 {paused&&<div className="callout info"><Icon name="pause"/><div><strong>Live updates paused</strong><p>Viewing the snapshot from {new Date(updated).toLocaleTimeString()}. The proxy keeps running.</p></div></div>}
 {!state&&token&&!error&&<div className="dashboard-skeleton" aria-label="Loading gateway"><div className="cards">{[1,2,3,4].map(value=><div key={value} className="skeleton"/>)}</div><div className="skeleton chart-skeleton"/></div>}
 {state&&<div className="view-content" key={view}>
 <div className="cards">
 {[
  {label:"Total requests",value:format(total),caption:"Since gateway startup",icon:"requests",color:"teal",series:points.map(value=>value.rate)},
  {label:"Request rate",value:latest?format(latest.rate):"—",unit:"req/s",caption:"From live counter deltas",icon:"pulse",color:"blue",series:points.map(value=>value.rate)},
  {label:"In flight",value:format(state.telemetry.active),caption:format(state.policies.max_in_flight)+" concurrent capacity",icon:"clock",color:"violet",series:[]},
  {label:"Blocked requests",value:format(state.telemetry.blocked),caption:total?(state.telemetry.blocked/total*100).toFixed(1)+"% of admissions":"Rejected before forwarding",icon:"shield",color:"amber",series:points.map(value=>value.blocked)}
 ].map(card=><article className={"stat-card "+card.color} key={card.label}><div className="stat-card-label"><span>{card.label}</span><span className="stat-icon"><Icon name={card.icon} size={17}/></span></div><div className="stat-card-value"><strong>{card.value}</strong>{card.unit&&<span>{card.unit}</span>}<Sparkline values={card.series} color={"var(--"+card.color+")"}/></div><div className="stat-card-caption">{card.caption}</div></article>)}
 </div>
 {(view==="Overview"||view==="Analysis")&&<Queue state={state}/>}
 {(state.dropped_events>0||(state.analysis?.dropped??0)>0)&&<div className="callout warning" role="alert"><Icon name="alert"/><div><strong>Capture coverage is incomplete</strong><p>{format(state.storage.dropped_batches)} omitted capture batches · {format(state.analysis?.dropped??0)} skipped analysis jobs.</p></div></div>}
 {(state.storage.pressure_rejections>0||(state.analysis?.pressure_rejections??0)>0)&&<div className="callout info"><Icon name="shield"/><div><strong>Admission protection is active</strong><p>{format(state.storage.pressure_rejections)} storage rejections · {format(state.analysis?.pressure_rejections??0)} analysis rejections. These requests were stopped before forwarding.</p></div></div>}
 {(!state.storage.healthy||state.analysis?.journal_healthy===false)&&<div className="callout danger" role="alert"><Icon name="database"/><div><strong>Persistence needs attention</strong><p>Pending batches remain in memory. Required admissions are paused until storage recovers.</p></div></div>}
 {(view==="Overview"||view==="Analysis")&&<div className="chart-grid"><Panel title="Traffic activity" description="Request and rejection rates · requests per second" action={<div className="segmented" aria-label="Chart window">{[30,60].map(value=><button key={value} aria-pressed={windowSize===value} onClick={()=>setWindowSize(value)}>{value} samples</button>)}</div>}>
 <div className="chart-legend"><span><i className="teal"/>Requests <b>{latest?format(latest.rate):"—"}/s</b></span><span><i className="amber"/>Blocked <b>{latest?format(latest.blocked):"—"}/s</b></span></div><TrafficChart points={points.slice(-windowSize)}/></Panel>
 <Panel title="Request latency" description="Completed requests · milliseconds" action={<span className="tag">SINCE STARTUP</span>}><LatencyChart buckets={state.telemetry.latency_buckets}/><div className="chart-foot"><span className="badge neutral">{format(state.telemetry.completed+state.telemetry.blocked+failed)} outcomes</span><span>{total?(failed/total*100).toFixed(2):"0.00"}% failures</span></div></Panel></div>}
 {(view==="Overview"||view==="Requests")&&<Requests state={state} select={select} compact={view==="Overview"}/>}
 {(view==="Overview"||view==="Upstreams")&&<Upstreams state={state}/>}
 {view==="Overview"&&<div className="overview-bottom"><section className="insight-card"><span className="insight-icon"><Icon name="analysis" size={23}/></span><div><span className="eyebrow">BACKGROUND INTELLIGENCE</span><h3>{state.model?format(state.model.parameter_count)+" parameters. Off the request path.":"Independent proxy operation."}</h3><p>{format(state.analysis?.finished??0)} completed analyses · {state.policies.model} mode · {format(state.storage.committed_batches)} persisted journeys</p></div><button className="button secondary" onClick={()=>setView("Analysis")}>Explore analysis<Icon name="arrow" size={14}/></button></section>
 <div className="overview-tools"><button className="text-button" disabled={busy} onClick={()=>void action("/cache/purge",{kind:"all"},"Stored cache entries cleared. Persistent decisions remain active.")}><Icon name="refresh" size={15}/>Clear caches</button><span>Updated {new Date(updated).toLocaleTimeString()} · Config {state.config_version.slice(0,8)}</span></div></div>}
 {view==="Decisions"&&<Decisions state={state} verdicts={verdicts} cancellations={cancellations} revoke={verdict=>setIntent({kind:"revoke",verdict})} select={select} busy={busy}/>}
 {view==="Analysis"&&<Diagnostics state={state}/>}
 {view==="Configuration"&&<Configuration state={state}/>}
 </div>}
 {selected&&state&&<Journey id={selected} events={events.length?events:selectedActive?.events??[]} active={selectedActive} close={()=>setSelected("")} block={()=>setIntent({kind:"block"})} canBlock={!!selectedEvent&&state.policies.decision_cache} canCancel={state.policies.cancellable_routes.includes(route)} cancel={()=>setIntent({kind:"cancel"})} error={detailError} loading={loadingDetail} busy={busy}/>}
 {intent&&state&&<ActionForm error={notice?.kind==="error"?notice.text:undefined} kind={intent.kind} scope={intent.kind==="revoke"?intent.verdict?.route+" · "+intent.verdict?.actor.slice(0,12):route+" · "+String(selectedEvent?.details.actor??selected).slice(0,12)} maxTtl={state.policies.deny_ttl_ms} busy={busy} submit={submit} close={()=>setIntent(null)}/>}
 <Toast notice={notice} close={dismiss}/>
 </Shell>;
}
