"use client";
import {useMemo,useState} from "react";
import type {Event,State} from "../lib/client";
import {Empty,Icon,Panel} from "./ui";
export function Requests({state,select,compact=false}:{state:State;select:(id:string)=>void;compact?:boolean}){
 const [search,setSearch]=useState(""),[filter,setFilter]=useState("all"),[page,setPage]=useState(0),[size,setSize]=useState(10);
 const rows=useMemo(()=>{
  const groups=new Map<string,Event>();
  for(const event of state.telemetry.recent)if(!groups.has(event.request_id))groups.set(event.request_id,event);
  for(const journey of state.journeys.items){const latest=journey.events.at(-1);if(latest&&!groups.has(journey.request_id))groups.set(journey.request_id,latest);}
  return [...groups.values()].sort((a,b)=>b.timestamp_ms-a.timestamp_ms)
   .filter(event=>(event.request_id+" "+event.stage+" "+String(event.details.route??"")).toLowerCase().includes(search.toLowerCase()))
   .filter(event=>filter==="all"||(filter==="blocked"?["blocked","rejected"].includes(event.stage):filter==="risk"?Number(event.details.risk_score)>.5:state.journeys.items.some(item=>item.request_id===event.request_id)));
 },[state,search,filter]);
 const pages=Math.max(1,Math.ceil(rows.length/size)),current=Math.min(page,pages-1),visible=rows.slice(current*size,(current+1)*size);
 return <Panel title="Request explorer" description="Follow captured traffic from admission to outcome." action={<span className="tag">{rows.length} journeys</span>} className={"requests "+(compact?"compact":"")}>
 <div className="table-toolbar"><div className="search-field"><Icon name="search" size={16}/><input aria-label="Search requests" placeholder="Search request ID, route or stage…" value={search} onChange={event=>{setSearch(event.target.value);setPage(0);}}/><kbd>/</kbd></div>
 <select aria-label="Filter requests" value={filter} onChange={event=>{setFilter(event.target.value);setPage(0);}}><option value="all">All requests</option><option value="active">In flight</option><option value="blocked">Blocked</option><option value="risk">Risk score &gt; 50%</option></select></div>
 <div className="table-scroll" tabIndex={0} aria-label="Captured requests table"><table><thead><tr><th scope="col">Request</th><th scope="col">Route</th><th scope="col">Status</th><th scope="col">Duration</th><th scope="col">Risk score</th><th scope="col">Received</th><th scope="col"><span className="sr-only">Details</span></th></tr></thead><tbody>
 {visible.map(event=><tr key={event.request_id}><td><button className="row-button mono" aria-label={"Inspect request "+event.request_id} onClick={()=>select(event.request_id)}><span className="request-dot"/>{event.request_id.slice(0,8)}<span className="muted">…</span></button></td><td><span className="route-name">{String(event.details.route??"—")}</span></td><td><span className={"badge "+(["blocked","rejected","failed"].includes(event.stage)?"danger":event.stage==="analyzed"?"violet":"success")}><i/>{event.stage.replaceAll("_"," ")}</span></td><td className="mono">{event.elapsed_ms} <span className="muted">ms</span></td><td>{event.details.risk_score==null?<span className="muted">—</span>:<span className={"risk "+(Number(event.details.risk_score)>.5?"high":"low")}><span className="risk-meter"><i style={{width:(Number(event.details.risk_score)*100)+"%"}}/></span>{(Number(event.details.risk_score)*100).toFixed(1)}%</span>}</td><td className="muted mono">{new Date(event.timestamp_ms).toLocaleTimeString()}</td><td><button className="icon-button small" aria-label={"Open journey "+event.request_id} onClick={()=>select(event.request_id)}><Icon name="arrow" size={15}/></button></td></tr>)}
 </tbody></table>{!rows.length&&<Empty title={search||filter!=="all"?"No matching requests":"Waiting for captured traffic"}>{search||filter!=="all"?"Try another request ID, route or filter.":"Forward traffic through a route with capture enabled to explore its journey."}</Empty>}</div>
 <div className="table-footer"><span>{rows.length?current*size+1:0}–{Math.min((current+1)*size,rows.length)} of {rows.length} recent journeys</span><div><label className="page-size">Rows <select aria-label="Rows per page" value={size} onChange={event=>{setSize(Number(event.target.value));setPage(0);}}>{[10,25,50].map(value=><option key={value}>{value}</option>)}</select></label><button className="icon-button previous" aria-label="Previous page" disabled={current===0} onClick={()=>setPage(current-1)}><Icon name="arrow" size={15}/></button><span>{current+1} / {pages}</span><button className="icon-button" aria-label="Next page" disabled={current===pages-1} onClick={()=>setPage(current+1)}><Icon name="arrow" size={15}/></button></div></div>
 </Panel>;
}
