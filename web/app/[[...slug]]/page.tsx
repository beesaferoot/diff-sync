import { Page } from "./client";

export function generateStaticParams() {
  return [{ slug: [""] }];
}

export default function CatchAllPage() {
  return <Page />;
}
