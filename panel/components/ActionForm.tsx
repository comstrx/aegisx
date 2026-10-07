"use client";
import {useState} from "react";
import {Dialog,Icon} from "./ui";
export function ActionForm({kind,scope,maxTtl,busy,submit,close,error}:{kind:"block"|"revoke"|"cancel";scope:string;maxTtl:number;busy:boolean;error?:string;submit:(reason:string,ttl:number)=>Promise<boolean>;close:()=>void}){
 const [reason,setReason]=useState(""),[seconds,setSeconds]=useState(Math.min(30,Math.floor(maxTtl/1000)));
 const title=kind==="block"?"Restrict future admissions":kind==="revoke"?"Revoke this restriction":"Request backend cancellation";
 return <Dialog close={()=>{if(!busy)close();}} label={title}><form onSubmit={async event=>{event.preventDefault();if(await submit(reason,seconds*1000))close();}}>
 <div className="modal-head"><span className="modal-icon"><Icon name="shield" size={24}/></span><button type="button" disabled={busy} className="icon-button" aria-label="Close action" onClick={close}><Icon name="close"/></button></div>
 <h2>{title}</h2><p className="muted">{kind==="block"?"Apply an expiring rule to this actor and route. An already-forwarded request continues normally.":kind==="revoke"?"Remove the stored decision and refresh its cache after the database commit. Other configured policies still apply.":"Ask the cooperating backend to stop related work at a safe checkpoint. This cannot undo completed effects."}</p>
 <div className="scope-box"><span>Scope</span><b className="mono">{scope}</b></div>
 {kind==="block"&&<div className="form-fields"><label>Reason<textarea aria-label="Restriction reason" required maxLength={128} rows={3} placeholder="Describe the observed behavior…" value={reason} onChange={event=>setReason(event.target.value)}/><small>Recorded with this operator decision.</small></label><label>Duration (seconds)<input aria-label="Restriction duration" type="number" min={1} max={Math.max(1,Math.floor(maxTtl/1000))} required value={seconds} onChange={event=>setSeconds(Number(event.target.value))}/><small>Maximum {Math.floor(maxTtl/1000)} seconds under the active policy.</small></label></div>}
 {error&&<div className="callout danger" role="alert"><Icon name="alert"/><p>{error}</p></div>}<div className="modal-actions"><button type="button" className="button secondary" disabled={busy} onClick={close}>Keep unchanged</button><button className="button primary" disabled={busy||kind==="block"&&!reason.trim()}>{busy?<><span className="spinner"/>Applying…</>:kind==="block"?"Apply restriction":kind==="revoke"?"Revoke decision":"Send cancellation request"}</button></div>
 </form></Dialog>;
}
