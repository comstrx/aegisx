"use client";
import {useEffect,useRef,useState} from "react";
import {api,type State,type Verdict,type Cancellation} from "./client";
export type Point={at:number;rate:number;blocked:number;failed:number};
export function useRuntime(token:string,prefix:string,paused=false) {
 const [state,setState]=useState<State|null>(null),[verdicts,setVerdicts]=useState<Verdict[]>([]),[cancellations,setCancellations]=useState<Cancellation[]>([]);
 const [points,setPoints]=useState<Point[]>([]),[error,setError]=useState(""),[updated,setUpdated]=useState(0);
 const refreshRef=useRef<()=>void>(()=>{}),pausedRef=useRef(paused);
 useEffect(()=>{pausedRef.current=paused;if(!paused)refreshRef.current();},[paused]);
 useEffect(()=>{
  setState(null);setPoints([]);setVerdicts([]);setCancellations([]);setError("");setUpdated(0);
  if(!token||!prefix)return;
  let stopped=false,running=false,previous:State|null=null;
  const controller=new AbortController();
  async function update(force=false) {
   if(stopped||running||(!force&&pausedRef.current))return;
   running=true;
   try {
    const signal=AbortSignal.any([controller.signal,AbortSignal.timeout(5000)]);
    const next=await api<State>(prefix,token,"/state",undefined,signal);
    const [decisions,actions]=await Promise.all([api<{items:Verdict[]}>(prefix,token,"/decisions",undefined,signal),api<{items:Cancellation[]}>(prefix,token,"/cancellations",undefined,signal)]);
    if(stopped)return;
    if(previous&&next.uptime_ms>previous.uptime_ms&&next.telemetry.total>=previous.telemetry.total) {
     const before=previous;
     const seconds=(next.uptime_ms-before.uptime_ms)/1000;
     setPoints(values=>[...values,{at:Date.now(),rate:(next.telemetry.total-before.telemetry.total)/seconds,blocked:Math.max(0,next.telemetry.blocked-before.telemetry.blocked)/seconds,failed:Math.max(0,next.telemetry.failed-before.telemetry.failed)/seconds}].slice(-60));
    } else setPoints([]);
    previous=next;setState(next);setVerdicts(decisions.items);setCancellations(actions.items);setUpdated(Date.now());setError("");
   } catch(error) {if(!stopped)setError((error as Error).message);}
   finally {running=false;}
  }
  refreshRef.current=()=>void update(true);
  void update();const timer=setInterval(()=>void update(),1000);
  return ()=>{stopped=true;controller.abort();clearInterval(timer);refreshRef.current=()=>{};};
 },[token,prefix]);
 return {state,verdicts,cancellations,points,error,updated,refresh:()=>refreshRef.current()};
}
