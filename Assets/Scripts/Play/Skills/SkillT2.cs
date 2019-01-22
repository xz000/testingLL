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
    public int bulletamount;
    public float force;
    public float damage;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;
    float firetime = 0.1f;
    float maxfiretime = 0.7f;
    Fix64Vector2 drt;
    bool firestart = false;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
        fireball.GetComponent<BombExplode>().sender = gameObject;
    }

    // Update is called once per frame
    void GoSkillT2()
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
        if (firestart && firetime >= 0.1f)
            FFF(drt.ToV2());
        firetime += Time.fixedDeltaTime;
    }

    public void Skill(Fix64Vector2 actionplacef)
    {
        Vector2 actionplace = actionplacef.ToV2();
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        Vector2 skilldirection = actionplace - gameObject.GetComponent<Rigidbody2D>().position;
    }

    public void FFF(Vector2 direction)
    {
        direction = Quaternion.AngleAxis(2 * (bulletamount - 2), Vector3.forward) * direction;
        int bnum = 0;
        while (bnum < bulletamount)
        {
            DoFire(gameObject.GetComponent<Rigidbody2D>().position + 0.5f * direction.normalized, direction.normalized * bulletspeed);
            direction = Quaternion.AngleAxis(-2, Vector3.forward) * direction;
            bnum++;
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
