using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillD4 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public float maxdistance = 10;
    public float maxtime = 2.5f;
    public float bulletspeed = 5;
    public GameObject fireball;
    public float force = 15;
    public float damage = 10;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void GoSkillD4()
    {
        if (skillavaliable)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            //MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    public void Skill(Fix64Vector2 actionplace)
    {
        Fix64Vector2 singplace = (Fix64Vector2)(Vector2)transform.position;
        Fix64Vector2 skilldirection = actionplace - singplace;
        float realdistance = Mathf.Min((float)skilldirection.Length(), maxdistance);
        if (realdistance <= 0.51)
        {
            return;
        }   //半径小于自身半径时不施法
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        bulletspeed = realdistance * 1.13f / (maxtime - 0.5f);
        Fix64Vector2 fp1 = skilldirection.normalized().CCWTurn(Fix64.Pi / (Fix64)4);
        DoFire(singplace, fp1 * (Fix64)bulletspeed, false);
        fp1 = fp1.CCWTurn(-Fix64.Pi / (Fix64)2);
        DoFire(singplace, fp1 * (Fix64)bulletspeed, true);
    }

    void DoFire(Fix64Vector2 fireplace, Fix64Vector2 speed2d, bool a)
    {
        
        fireball.GetComponent<BananaScript>().sender = gameObject;
        fireball.GetComponent<BananaScript>().bombdamage = (Fix64)damage;
        fireball.GetComponent<BananaScript>().maxtime = maxtime;
        GameObject bullet = Instantiate(fireball, fireplace.ToV2(), Quaternion.identity);
        bullet.GetComponent<Rigidbody2D>().velocity = speed2d.ToV2();
        if (a)
            bullet.GetComponent<BananaScript>().setmm(2);
        else
            bullet.GetComponent<BananaScript>().setmm(-2);
        //bullet.GetComponent<BombExplode>().bombpower = force;
        //bullet.GetComponent<BombExplode>().maxtime = maxdistance / bulletspeed;
    }

    void SkillD4SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }
}
