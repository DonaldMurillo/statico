import { Injectable } from '@nestjs/common';
import { CreateUserDto } from './dto/create-user.dto';

@Injectable()
export class UsersService {
  private users: CreateUserDto[] = [];

  findAll(): CreateUserDto[] {
    return this.users;
  }

  findOne(id: string): CreateUserDto | undefined {
    return this.users[Number(id)];
  }

  create(dto: CreateUserDto): CreateUserDto {
    this.users.push(dto);
    return dto;
  }
}
