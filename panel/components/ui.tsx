"use client";
import {useEffect,useRef,useState,type ReactNode} from "react";
export function Icon({name,size=18}:{name:string;size?:number}) {
 const paths:Record<string,ReactNode>={
  overview:<><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="4" rx="1.5"/><rect x="14" y="11" width="7" height="10" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/></>,
  requests:<><path d="M3 7h17m-4-4 4 4-4 4M21 17H4m4-4-4 4 4 4"/></>,
  shield:<><path d="M12 3 4 6v6c0 5 8 9 8 9s8-4 8-9V6Z"/><path d="m8 12 3 3 5-6"/></>,
  upstream:<><rect x="4" y="3" width="16" height="6" rx="2"/><rect x="4" y="15" width="16" height="6" rx="2"/><path d="M8 6h.01M8 18h.01M12 9v6"/></>,
  analysis:<><path d="m12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5Z"/></>,
  settings:<><path d="M4 7h16M4 17h16"/><circle cx="9" cy="7" r="3"/><circle cx="16" cy="17" r="3"/></>,
  search:<><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/></>,
  arrow:<path d="m9 5 7 7-7 7"/>,
  check:<path d="m5 12 4 4L19 6"/>,
  close:<path d="M6 6 18 18M6 18 18 6"/>,
  alert:<><path d="m12 3 10 18H2Z"/><path d="M12 9v5m0 3v.01"/></>,
  clock:<><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></>,
  pulse:<path d="M2 12h5l3-8 4 16 3-8h5"/>,
  database:<><ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v14c0 4 16 4 16 0V5M4 12c0 4 16 4 16 0"/></>,
  sun:<><circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1 1m12 12 1 1M5 19l1-1M18 6l1-1"/></>,
  moon:<path d="M20 15A9 9 0 0 1 9 4a9 9 0 1 0 11 11Z"/>,
  refresh:<><path d="M20 7v5h-5M4 17v-5h5"/><path d="M6 7a7 7 0 0 1 12-2l2 3M4 16l2 3a7 7 0 0 0 12-2"/></>,
  download:<><path d="M12 3v12m-4-4 4 4 4-4M4 16v5h16v-5"/></>,
  pause:<><path d="M8 5v14M16 5v14"/></>,
  play:<path d="m8 4 12 8-12 8Z"/>,
  exit:<><path d="M10 4H4v16h6M9 12h12m-5-5 5 5-5 5"/></>,
  inbox:<><path d="M3 13 6 4h12l3 9v7H3Z"/><path d="M3 13h5l2 3h4l2-3h5"/></>,
  key:<><circle cx="8" cy="9" r="5"/><path d="m12 13 8 8m-2-2 3-3m-6 0 3-3"/></>,
  copy:<><rect x="8" y="8" width="12" height="13" rx="2"/><path d="M16 8V3H3v13h5"/></>,
 };
 return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]??paths.overview}</svg>;
}
export function Logo(){return <span className="brand"><span className="brand-mark"><svg viewBox="0 0 32 32" aria-hidden="true"><path d="M16 3 29 9v10L16 29 3 19V9Z" fill="currentColor"/><path d="m9 21 7-13 7 13h-5l-2-4-2 4Z" fill="var(--sidebar)"/></svg></span><span>Aegis<span className="brand-x">X</span><small>INTELLIGENT GATEWAY</small></span></span>}
export function Empty({icon="inbox",title,children,action}:{icon?:string;title:string;children?:ReactNode;action?:ReactNode}) {
 return <div className="empty-state"><span className="empty-icon"><Icon name={icon} size={24}/></span><strong>{title}</strong>{children&&<p>{children}</p>}{action}</div>;
}
export function Panel({title,description,action,children,className=""}:{title:string;description?:string;action?:ReactNode;children:ReactNode;className?:string}){
 return <section className={"surface "+className}><div className="section-title"><div><h2>{title}</h2>{description&&<p>{description}</p>}</div>{action}</div>{children}</section>;
}
export type Notice={text:string;kind:"success"|"error"|"info";id:number};
export function Toast({notice,close}:{notice:Notice|null;close:()=>void}){
 const [displayed,setDisplayed]=useState<Notice|null>(notice),[exiting,setExiting]=useState(false);
 useEffect(()=>{if(notice){setDisplayed(notice);setExiting(false);return;}setExiting(true);const id=setTimeout(()=>setDisplayed(null),180);return()=>clearTimeout(id);},[notice]);
 useEffect(()=>{if(!notice||notice.kind==="error")return;const id=setTimeout(close,6500);return()=>clearTimeout(id);},[notice,close]);
 return <div className="toast-region" aria-live="polite" aria-atomic="true">{displayed&&<div className={"toast "+displayed.kind+(exiting?" exiting":"")} role={displayed.kind==="error"?"alert":"status"} key={displayed.id}><span className="toast-icon"><Icon name={displayed.kind==="error"?"alert":"check"}/></span><div><strong>{displayed.kind==="error"?"Action could not be completed":displayed.kind==="success"?"Change applied":"Gateway update"}</strong><p>{displayed.text}</p></div><button className="icon-button" aria-label="Dismiss notification" onClick={close}><Icon name="close"/></button></div>}</div>;
}
export function Dialog({children,close,label,wide=false}:{children:ReactNode;close:()=>void;label:string;wide?:boolean}){
 const ref=useRef<HTMLDialogElement>(null);
 useEffect(()=>{const element=ref.current;element?.showModal();return()=>{element?.close();};},[]);
 return <dialog ref={ref} className={wide?"drawer":"modal"} aria-label={label} onCancel={event=>{event.preventDefault();close();}} onClick={event=>{if(event.target===event.currentTarget)close();}}><div className="dialog-content">{children}</div></dialog>;
}
export const format=(value:number)=>new Intl.NumberFormat("en",{notation:value>99999?"compact":"standard",maximumFractionDigits:1}).format(value);
