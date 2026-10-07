"use client";
import type {ReactNode} from "react";
import {Icon,Logo} from "./ui";
export const views=["Overview","Requests","Decisions","Upstreams","Analysis","Configuration"] as const;
export type View=typeof views[number];
const icons=["overview","requests","shield","upstream","analysis","settings"];
export function Shell({view,navigate,children,online,paused,connected,version,dark,toggleTheme,active,disconnect}:{view:View;navigate:(view:View)=>void;children:ReactNode;online:boolean;paused:boolean;connected:boolean;version?:string;dark:boolean;toggleTheme:()=>void;active:number;disconnect:()=>void}){
 return <div className="app-shell" data-theme={dark?"dark":"light"}>
 <a className="skip-link" href="#main-content">Skip to content</a>
 <aside className="sidebar"><a href="/" aria-label="AegisX home"><Logo/></a>
 <div className="instance-selector"><span className="instance-glyph"><Icon name="upstream"/></span><div><strong>Local gateway</strong><small>Single instance</small></div><span className="tag neutral">LOCAL</span></div>
 <div className="nav-label">WORKSPACE</div><nav aria-label="Main navigation">{views.map((item,index)=><button key={item} aria-current={view===item?"page":undefined} className={"nav "+(view===item?"active":"")} onClick={()=>navigate(item)}><Icon name={icons[index]}/><span>{item}</span>{item==="Requests"&&active>0&&<em>{formatActive(active)}</em>}{item===view&&<span className="nav-indicator"/>}</button>)}</nav>
 <div className="side-note"><span className="eyebrow"><span className="pulse-dot"/> BUILT TO STAY LOCAL</span><p>Your traffic.<br/>Your policies. Your control.</p><div className="local-chip"><Icon name="shield" size={14}/> No cloud connection required</div></div>
 <div className="side-bottom"><span className="avatar">AX</span><div><strong>AegisX engine</strong><small>{version?"Version "+version:"Not connected"}</small></div>{connected&&<button className="icon-button" aria-label="Disconnect" title="Disconnect" onClick={disconnect}><Icon name="exit"/></button>}</div>
 </aside>
 <div className="workspace"><div className="topbar"><div className="breadcrumbs"><Icon name="overview" size={15}/><span>Workspace</span><Icon name="arrow" size={12}/><b>{view}</b></div><div className="topbar-actions"><span className={"connection-status "+(online&&!paused?"online":"")}><i/>{paused?"Paused":online?"Live · 1s refresh":connected?"Connecting":"Not connected"}</span><span className="topbar-separator"/><button className="icon-button" aria-label={dark?"Switch to light theme":"Switch to dark theme"} onClick={toggleTheme}><Icon name={dark?"sun":"moon"}/></button><button className="icon-button mobile-disconnect" hidden={!connected} disabled={!connected} aria-label="Disconnect" onClick={disconnect}><Icon name="exit"/></button><span className="user-avatar" title="Local operator">OP</span></div></div>
 <main id="main-content" tabIndex={-1}>{children}</main><div className="workspace-footer"><span><Icon name="shield" size={12}/> Local by design.</span><span>AegisX Gateway Control</span></div></div></div>;
}
function formatActive(value:number){return value>999?"999+":value;}
