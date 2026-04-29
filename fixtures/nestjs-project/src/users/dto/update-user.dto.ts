export class UpdateUserDto {
  name?: string;
  email?: string;
  role?: 'admin' | 'user';
  active?: boolean;
}
