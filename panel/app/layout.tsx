import type { Metadata } from "next";
import "./style.css";

export const metadata: Metadata = {
    title: "AegisX · Runtime",
    description: "Local traffic, policy and upstream visibility",
};

export default function Layout ({ children }: Readonly<{ children: React.ReactNode }>) {
    return <html lang="en"><body>{children}</body></html>;
}
