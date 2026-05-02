import { Routes } from '@angular/router';
import { HomeComponent } from './pages/home';
import { DocsLayoutComponent } from './pages/docs-layout';
import { DocViewerComponent } from './pages/doc-viewer';
import { docResolver } from './resolvers/doc.resolver';

export const routes: Routes = [
  { path: '', component: HomeComponent },
  {
    path: 'docs',
    component: DocsLayoutComponent,
    children: [
      { path: '', redirectTo: 'getting-started', pathMatch: 'full' },
      {
        path: ':slug',
        component: DocViewerComponent,
        resolve: { doc: docResolver },
      },
    ],
  },
  { path: '**', redirectTo: '' },
];
