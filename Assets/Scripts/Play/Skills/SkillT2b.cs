using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillT2b : MonoBehaviour
{

    public CooldownImage MyImageScript;
    public float maxdistance;
    public float bulletspeed;
    public GameObject fireball;
    public int bulletamount = 4;
    public float force;
    public float damage;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;
    Fix64Vector2 spf;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
        fireball.GetComponent<BombExplode>().sender = gameObject;
    }

    void GoSkillT2b()
    {
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
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
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        spf = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        Fix64Vector2 skilldirection = (actionplace - spf).normalized();
        FFF(skilldirection);
    }

    void FFF(Fix64Vector2 direction)
    {
        direction = direction.CCWTurn(-Fix64.Pi / (Fix64)6);
        for (int bnum = 0; bnum < bulletamount; bnum++)
        {
            DoFire((spf + (direction * (Fix64)0.6)).ToV2(), (direction * (Fix64)bulletspeed).ToV2());
            direction = direction.CCWTurn(Fix64.Pi / (Fix64)8);
        }
    }

    void DoFire(Vector2 fireplace, Vector2 speed2d)
    {
        GameObject bullet;
        //fireball.GetComponent<BombExplode>().sender = gameObject;
        bullet = Instantiate(fireball, fireplace, Quaternion.identity);
        //bullet.GetComponent<BombExplode>().bombpower = force;
        bullet.GetComponent<BombExplode>().bombdamage = damage;
        bullet.GetComponent<Rigidbody2D>().velocity = speed2d;
        //bullet.GetComponent<BombExplode>().maxtime = maxdistance / bulletspeed;
    }

    void SkillT2bSetLevel(int i)
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
