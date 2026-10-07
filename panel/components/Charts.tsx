"use client";
import {useId,useState} from "react";
import type {Point} from "../lib/runtime";
import {format} from "./ui";
export function Sparkline({values,color="var(--accent)"}:{values:number[];color?:string}){
 if(values.length<2)return <span className="sparkline-placeholder"/>;
 const max=Math.max(1,...values),path=values.map((value,index)=>(index/(values.length-1)*100)+","+(28-value/max*25)).join(" ");
 return <svg className="sparkline" viewBox="0 0 100 32" preserveAspectRatio="none" aria-hidden="true"><polyline points={path} fill="none" stroke={color} strokeWidth="1.6" vectorEffect="non-scaling-stroke"/></svg>;
}
export function TrafficChart({points}:{points:Point[]}){
 const [hover,setHover]=useState<number|null>(null);
 const id=useId().replaceAll(":",""),max=Math.max(10,Math.ceil(Math.max(0,...points.map(point=>point.rate))/10)*10);
 const x=(index:number)=>index/Math.max(1,points.length-1)*600;
 const y=(value:number)=>180-value/max*166;
 const line=(key:"rate"|"blocked")=>points.map((point,index)=>x(index)+","+y(point[key])).join(" ");
 const chosen=hover==null?null:points[Math.min(points.length-1,hover)];
 return <div className="chart-wrap" tabIndex={0} role="group" aria-label="Traffic chart. Use left and right arrows to inspect samples."
 onKeyDown={event=>{if(event.key==="ArrowRight"||event.key==="ArrowLeft"){event.preventDefault();setHover(value=>Math.max(0,Math.min(points.length-1,(value??points.length-1)+(event.key==="ArrowRight"?1:-1))));}}}
 onMouseLeave={()=>setHover(null)}>
 <div className="chart-scale">{[max,max*.75,max*.5,max*.25,0].map(value=><span key={value}>{format(value)}</span>)}</div>
 <svg viewBox="0 0 600 195" preserveAspectRatio="none" onMouseMove={event=>{const rect=event.currentTarget.getBoundingClientRect();setHover(Math.round((event.clientX-rect.left)/rect.width*Math.max(0,points.length-1)));}}>
 <defs><linearGradient id={id} x1="0" y1="0" x2="0" y2="1"><stop stopColor="var(--accent)" stopOpacity=".26"/><stop offset="1" stopColor="var(--accent)" stopOpacity=".025"/></linearGradient></defs>
 {[0,.25,.5,.75,1].map(value=><line key={value} x1="0" x2="600" y1={y(value*max)} y2={y(value*max)} stroke="var(--line)" strokeDasharray="3 5"/>)}
 {points.length>1&&<><polygon points={"0,180 "+line("rate")+" 600,180"} fill={"url(#"+id+")"}/><polyline points={line("rate")} fill="none" stroke="var(--accent)" className="traffic-line" strokeWidth="2.5" vectorEffect="non-scaling-stroke"/><polyline points={line("blocked")} fill="none" stroke="var(--warning)" strokeWidth="1.6" vectorEffect="non-scaling-stroke"/></>}
 {chosen&&hover!=null&&<><line x1={x(Math.min(hover,points.length-1))} x2={x(Math.min(hover,points.length-1))} y1="0" y2="180" stroke="var(--muted)" strokeDasharray="3 3"/><circle cx={x(Math.min(hover,points.length-1))} cy={y(chosen.rate)} r="4" fill="var(--accent)" stroke="var(--surface)" strokeWidth="2"/></>}
 </svg>
 {points.length<2&&<div className="chart-empty"><span className="pulse-dot"/>Collecting live samples…</div>}
 {chosen&&<div className="chart-tooltip" role="status"><strong>{new Date(chosen.at).toLocaleTimeString()}</strong><span>Requests <b>{format(chosen.rate)}/s</b></span><span>Blocked <b>{format(chosen.blocked)}/s</b></span></div>}
 <div className="chart-labels"><span>{points[0]?new Date(points[0].at).toLocaleTimeString():"Waiting for traffic"}</span><span>{points.length} live samples</span><span>Now</span></div>
 <span className="sr-only">Latest: {format(points.at(-1)?.rate??0)} requests per second; {format(points.at(-1)?.blocked??0)} blocked per second.</span>
 </div>;
}
export function LatencyChart({buckets}:{buckets:number[]}){
 const max=Math.max(1,...buckets),labels=["≤1","1–5","5–10","10–25","25–50","50–100","100–500",">500"];
 return <div className="histogram" role="img" aria-label={"Completed request latency in milliseconds: "+labels.map((label,index)=>label+": "+(buckets[index]??0)).join(", ")}>
 {labels.map((label,index)=><div className="hist-column" key={label} title={label+" ms: "+(buckets[index]??0)+" requests"}><span>{format(buckets[index]??0)}</span><div className="hist-track"><i data-empty={(buckets[index]??0)===0} style={{height:(buckets[index]??0)/max*100+"%"}}/></div><small>{label}</small></div>)}</div>;
}
