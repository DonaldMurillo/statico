import { Routes } from '@angular/router';
import { authGuard } from './guards/auth.guard';

export const routes: Routes = [
  {
    path: '',
    loadComponent: () =>
      import('./components/header.component').then(
        (m) => m.HeaderComponent
      ),
  },
  {
    path: 'dashboard',
    canActivate: [authGuard],
    loadComponent: () =>
      import('./components/footer.component').then(
        (m) => m.FooterComponent
      ),
  },
  {
    path: '**',
    redirectTo: '',
  },
];
