using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillT2 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public float maxdistance;
    public float bulletspeed;
    public GameObject fireball;
    public float force;
    public float damage;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;
    float firetime = 0.1f;
    int firetimes = 0;
    public int maxfiretime = 7;
    Fix64Vector2 drt;
    bool firestart = false;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
        fireball.GetComponent<BombExplode>().sender = gameObject;
    }

    void GoSkillT2()
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
            skillavaliable = true;
        else
            currentcooldown += Time.fixedDeltaTime;
        if (firestart)
        {
            firetime += Time.fixedDeltaTime;
            if (firetime >= 0.1f)
                FFF();
        }
    }

    public void Skill(Fix64Vector2 actionplace)
    {
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        drt = (actionplace - (Fix64Vector2)GetComponent<Rigidbody2D>().position).normalized().CCWTurn((Fix64)maxfiretime * Fix64.Pi / (Fix64)90);
        firetimes = 0;
        firestart = true;
    }

    public void FFF()
    {
        DoFire(((Fix64Vector2)GetComponent<Rigidbody2D>().position + (Fix64)0.6 * drt).ToV2(), (drt * (Fix64)bulletspeed).ToV2());
        firetime -= 0.1f;
        if (firetimes > maxfiretime)
        {
            firestart = false;
            return;
        }
        firetimes++;
        drt = drt.CCWTurn(-Fix64.Pi / (Fix64)90);
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

    void SkillT2SetLevel(int i)
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
